//! Les appels audio et vidéo d'Alley : WebRTC de pair à pair (maillage, un
//! lien par participant), média chiffré par DTLS-SRTP.
//!
//! La mise en relation (offres, réponses, candidats ICE) passe par le relais
//! chiffré et signé du serveur Alley : le serveur ne peut ni lire ni modifier
//! les empreintes DTLS, donc ni écouter ni s'intercaler. Un TURN éventuel ne
//! relaie que du chiffré.
//!
//! Pour éviter les offres croisées, c'est toujours le participant à
//! l'identifiant le plus petit qui appelle l'autre ; allumer ou couper la
//! caméra ne fait ensuite que changer de piste (`replaceTrack`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, JSON};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AnalyserNode, AudioContext, MediaStream, MediaStreamConstraints, MediaStreamTrack,
    RtcConfiguration, RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent,
    RtcPeerConnectionState, RtcRtpSender, RtcRtpTransceiver, RtcRtpTransceiverDirection,
    RtcRtpTransceiverInit, RtcSessionDescriptionInit, RtcSignalingState, RtcTrackEvent,
};

use crate::dom::{call as js_call, get, window};

type Listener = Closure<dyn FnMut(JsValue)>;

pub struct Peer {
    pub pc: RtcPeerConnection,
    offerer: bool,
    pub stream: MediaStream,
    video: Option<RtcRtpSender>,
    pending: Vec<Value>,
    _listeners: Vec<Listener>,
}

pub struct Call {
    pub me: String,
    ice: JsValue,
    signal: Rc<dyn Fn(String, Value)>,
    on_change: Rc<dyn Fn()>,
    pub peers: HashMap<String, Peer>,
    pub local: Option<MediaStream>,
    pub camera: Option<MediaStream>,
    pub screen: Option<MediaStream>,
    pub muted: bool,
    audio: Option<AudioContext>,
    levels: HashMap<String, AnalyserNode>,
}

pub type CallRef = Rc<RefCell<Call>>;

impl Call {
    pub fn new(
        me: String,
        ice: &Value,
        signal: impl Fn(String, Value) + 'static,
        on_change: impl Fn() + 'static,
    ) -> CallRef {
        Rc::new(RefCell::new(Call {
            me,
            ice: JSON::parse(&ice.to_string()).unwrap_or_else(|_| Array::new().into()),
            signal: Rc::new(signal),
            on_change: Rc::new(on_change),
            peers: HashMap::new(),
            local: None,
            camera: None,
            screen: None,
            muted: false,
            audio: None,
            levels: HashMap::new(),
        }))
    }

    pub fn video_track(&self) -> Option<MediaStreamTrack> {
        let stream = self.screen.as_ref().or(self.camera.as_ref())?;
        stream.get_video_tracks().get(0).dyn_into().ok()
    }

    fn audio_track(&self) -> Option<MediaStreamTrack> {
        self.local
            .as_ref()?
            .get_audio_tracks()
            .get(0)
            .dyn_into()
            .ok()
    }

    /// Qui parle : niveau sonore mesuré localement.
    pub fn speaking(&self, id: &str) -> bool {
        if id == self.me && self.muted {
            return false;
        }
        let Some(analyser) = self.levels.get(id) else {
            return false;
        };
        let mut data = vec![0u8; analyser.fft_size() as usize];
        analyser.get_byte_time_domain_data(&mut data);
        let sum: f64 = data.iter().map(|v| (*v as f64 - 128.0).powi(2)).sum();
        (sum / data.len().max(1) as f64).sqrt() > 6.0
    }

    pub fn can_share_screen() -> bool {
        window()
            .navigator()
            .media_devices()
            .is_ok_and(|m| !get(&m, "getDisplayMedia").is_undefined())
    }

    fn changed(&self) {
        let f = self.on_change.clone();
        f();
    }
}

fn watch_level(call: &CallRef, id: &str, stream: &MediaStream) {
    let mut c = call.borrow_mut();
    if c.audio.is_none() {
        c.audio = AudioContext::new().ok();
    }
    let Some(ctx) = c.audio.clone() else { return };
    let (Ok(source), Ok(analyser)) = (
        ctx.create_media_stream_source(stream),
        ctx.create_analyser(),
    ) else {
        return;
    };
    analyser.set_fft_size(512);
    let _ = source.connect_with_audio_node(&analyser);
    c.levels.insert(id.to_owned(), analyser);
}

async fn user_media(audio: bool, video: bool) -> Result<MediaStream, JsValue> {
    let devices = window().navigator().media_devices()?;
    let constraints = MediaStreamConstraints::new();
    if audio {
        constraints.set_audio(&JSON::parse(
            r#"{"echoCancellation":true,"noiseSuppression":true,"autoGainControl":true}"#,
        )?);
    }
    if video {
        constraints.set_video(&JSON::parse(
            r#"{"width":{"ideal":1280},"height":{"ideal":720},"facingMode":"user"}"#,
        )?);
    }
    let stream = JsFuture::from(devices.get_user_media_with_constraints(&constraints)?).await?;
    Ok(stream.unchecked_into())
}

/// Ouvre le micro (le geste de l'utilisateur est requis sur iOS).
pub async fn start(call: &CallRef) -> Result<(), JsValue> {
    let stream = user_media(true, false).await?;
    let me = call.borrow().me.clone();
    watch_level(call, &me, &stream);
    call.borrow_mut().local = Some(stream);
    call.borrow().changed();
    Ok(())
}

/// Les participants annoncés dans le salon : on se relie à chacun.
pub fn sync(call: &CallRef, participants: &[String]) {
    let (me, known): (String, Vec<String>) = {
        let c = call.borrow();
        (c.me.clone(), c.peers.keys().cloned().collect())
    };
    for id in participants {
        if *id != me && !known.contains(id) {
            connect(call, id);
        }
    }
    for id in known {
        if !participants.contains(&id) {
            drop_peer(call, &id);
        }
    }
}

fn signal(call: &CallRef, to: &str, data: Value) {
    let f = call.borrow().signal.clone();
    f(to.to_owned(), data);
}

fn sdp_json(pc: &RtcPeerConnection) -> Value {
    pc.local_description()
        .and_then(|d| JSON::stringify(&d).ok())
        .and_then(|s| s.as_string())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

fn connect(call: &CallRef, id: &str) {
    let cfg = RtcConfiguration::new();
    cfg.set_ice_servers(&call.borrow().ice);
    let Ok(pc) = RtcPeerConnection::new_with_configuration(&cfg) else {
        return;
    };
    let offerer = call.borrow().me.as_str() < id;
    let stream = MediaStream::new().expect("flux");
    let mut listeners: Vec<Listener> = Vec::new();

    let (weak, to) = (Rc::downgrade(call), id.to_owned());
    let on_ice = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let e: RtcPeerConnectionIceEvent = e.unchecked_into();
        let (Some(c), Some(candidate)) = (weak.upgrade(), e.candidate()) else {
            return;
        };
        let json = JSON::stringify(&candidate.to_json())
            .ok()
            .and_then(|s| s.as_string())
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        if let Some(json) = json {
            signal(&c, &to, json!({ "ice": json }));
        }
    });
    pc.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
    listeners.push(on_ice);

    let (weak, from, s) = (Rc::downgrade(call), id.to_owned(), Clone::clone(&stream));
    let on_track = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let e: RtcTrackEvent = e.unchecked_into();
        let Some(c) = weak.upgrade() else { return };
        let track = e.track();
        if track.kind() == "video"
            && let Some(peer) = c.borrow_mut().peers.get_mut(&from)
        {
            peer.video = Some(e.transceiver().sender());
        }
        s.add_track(&track);
        let weak2 = Rc::downgrade(&c);
        let refresh = Closure::<dyn FnMut(JsValue)>::new(move |_| {
            if let Some(c) = weak2.upgrade() {
                c.borrow().changed();
            }
        });
        for ev in ["ended", "mute", "unmute"] {
            let _ = track.add_event_listener_with_callback(ev, refresh.as_ref().unchecked_ref());
        }
        refresh.forget();
        if track.kind() == "audio"
            && let Ok(only) = MediaStream::new_with_tracks(&Array::of1(&track))
        {
            watch_level(&c, &from, &only);
        }
        c.borrow().changed();
    });
    pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
    listeners.push(on_track);

    let (weak, peer_id) = (Rc::downgrade(call), id.to_owned());
    let on_state = Closure::<dyn FnMut(JsValue)>::new(move |_| {
        let Some(c) = weak.upgrade() else { return };
        let failed = c.borrow().peers.get(&peer_id).is_some_and(|p| {
            p.offerer && p.pc.connection_state() == RtcPeerConnectionState::Failed
        });
        if failed {
            let (c2, id2) = (c.clone(), peer_id.clone());
            crate::dom::spawn(async move { offer(&c2, &id2, true).await });
        }
        c.borrow().changed();
    });
    pc.set_onconnectionstatechange(Some(on_state.as_ref().unchecked_ref()));
    listeners.push(on_state);

    let mut video = None;
    if offerer {
        let c = call.borrow();
        match (c.audio_track(), c.local.as_ref()) {
            (Some(audio), Some(local)) => {
                pc.add_track_0(&audio, local);
            }
            _ => {
                let init = RtcRtpTransceiverInit::new();
                init.set_direction(RtcRtpTransceiverDirection::Recvonly);
                pc.add_transceiver_with_str_and_init("audio", &init);
            }
        }
        let init = RtcRtpTransceiverInit::new();
        init.set_direction(RtcRtpTransceiverDirection::Sendrecv);
        let sender = pc
            .add_transceiver_with_str_and_init("video", &init)
            .sender();
        if let Some(track) = c.video_track() {
            let _ = sender.replace_track(Some(&track));
        }
        video = Some(sender);
    }
    call.borrow_mut().peers.insert(
        id.to_owned(),
        Peer {
            pc,
            offerer,
            stream,
            video,
            pending: Vec::new(),
            _listeners: listeners,
        },
    );
    if offerer {
        let (c, id) = (call.clone(), id.to_owned());
        crate::dom::spawn(async move { offer(&c, &id, false).await });
    }
    call.borrow().changed();
}

async fn offer(call: &CallRef, id: &str, restart: bool) {
    let Some(pc) = call.borrow().peers.get(id).map(|p| p.pc.clone()) else {
        return;
    };
    let options = web_sys::RtcOfferOptions::new();
    options.set_ice_restart(restart);
    let Ok(offer) = JsFuture::from(pc.create_offer_with_rtc_offer_options(&options)).await else {
        return;
    };
    if JsFuture::from(pc.set_local_description(offer.unchecked_ref()))
        .await
        .is_err()
    {
        return;
    }
    signal(call, id, json!({ "sdp": sdp_json(&pc) }));
}

fn description(v: &Value) -> Option<RtcSessionDescriptionInit> {
    JSON::parse(&v.to_string()).ok().map(JsCast::unchecked_into)
}

async fn add_ice(pc: &RtcPeerConnection, ice: &Value) {
    if let Ok(init) = JSON::parse(&ice.to_string()) {
        let init: RtcIceCandidateInit = init.unchecked_into();
        let _ =
            JsFuture::from(pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&init))).await;
    }
}

async fn flush(call: &CallRef, id: &str) {
    let Some((pc, pending)) = call
        .borrow_mut()
        .peers
        .get_mut(id)
        .map(|p| (p.pc.clone(), std::mem::take(&mut p.pending)))
    else {
        return;
    };
    for ice in pending {
        add_ice(&pc, &ice).await;
    }
}

/// Un signal reçu (déjà déchiffré et vérifié).
pub async fn receive(call: &CallRef, from: &str, data: Value) {
    if !call.borrow().peers.contains_key(from) {
        connect(call, from);
    }
    let Some((pc, offerer)) = call
        .borrow()
        .peers
        .get(from)
        .map(|p| (p.pc.clone(), p.offerer))
    else {
        return;
    };
    let kind = data["sdp"]["type"].as_str().unwrap_or_default().to_owned();
    if kind == "offer" && !offerer {
        let Some(desc) = description(&data["sdp"]) else {
            return;
        };
        if JsFuture::from(pc.set_remote_description(&desc))
            .await
            .is_err()
        {
            return;
        }
        let (audio, video) = {
            let c = call.borrow();
            (c.audio_track(), c.video_track())
        };
        for t in pc.get_transceivers().iter() {
            let t: RtcRtpTransceiver = t.unchecked_into();
            t.set_direction(RtcRtpTransceiverDirection::Sendrecv);
            if t.receiver().track().kind() == "audio" {
                if let Some(a) = &audio {
                    let _ = JsFuture::from(t.sender().replace_track(Some(a))).await;
                }
            } else {
                let sender = t.sender();
                let _ = JsFuture::from(sender.replace_track(video.as_ref())).await;
                if let Some(p) = call.borrow_mut().peers.get_mut(from) {
                    p.video = Some(sender);
                }
            }
        }
        let Ok(answer) = JsFuture::from(pc.create_answer()).await else {
            return;
        };
        if JsFuture::from(pc.set_local_description(answer.unchecked_ref()))
            .await
            .is_err()
        {
            return;
        }
        signal(call, from, json!({ "sdp": sdp_json(&pc) }));
        flush(call, from).await;
    } else if kind == "answer"
        && offerer
        && pc.signaling_state() == RtcSignalingState::HaveLocalOffer
    {
        let Some(desc) = description(&data["sdp"]) else {
            return;
        };
        let _ = JsFuture::from(pc.set_remote_description(&desc)).await;
        flush(call, from).await;
    } else if !data["ice"].is_null() {
        // Un candidat peut arriver avant l'offre : on le garde pour après.
        if pc.remote_description().is_some() {
            add_ice(&pc, &data["ice"]).await;
        } else if let Some(p) = call.borrow_mut().peers.get_mut(from) {
            p.pending.push(data["ice"].clone());
        }
    }
}

pub fn drop_peer(call: &CallRef, id: &str) {
    let peer = {
        let mut c = call.borrow_mut();
        c.levels.remove(id);
        c.peers.remove(id)
    };
    if let Some(p) = peer {
        p.pc.close();
    }
    call.borrow().changed();
}

/// Change la piste vidéo envoyée à tous (caméra, écran ou rien).
async fn refresh_video(call: &CallRef) {
    let (track, senders) = {
        let c = call.borrow();
        (
            c.video_track(),
            c.peers
                .values()
                .filter_map(|p| p.video.clone())
                .collect::<Vec<_>>(),
        )
    };
    for s in senders {
        let _ = JsFuture::from(s.replace_track(track.as_ref())).await;
    }
    call.borrow().changed();
}

fn stop_stream(s: &Option<MediaStream>) {
    if let Some(s) = s {
        for t in s.get_tracks().iter() {
            t.unchecked_into::<MediaStreamTrack>().stop();
        }
    }
}

pub fn toggle_mute(call: &CallRef) {
    let mut c = call.borrow_mut();
    c.muted = !c.muted;
    if let Some(local) = &c.local {
        for t in local.get_audio_tracks().iter() {
            t.unchecked_into::<MediaStreamTrack>().set_enabled(!c.muted);
        }
    }
    drop(c);
    call.borrow().changed();
}

pub async fn toggle_camera(call: &CallRef) -> Result<(), JsValue> {
    let camera = call.borrow_mut().camera.take();
    if camera.is_some() {
        stop_stream(&camera);
    } else {
        let stream = user_media(false, true).await?;
        call.borrow_mut().camera = Some(stream);
    }
    refresh_video(call).await;
    Ok(())
}

pub async fn toggle_screen(call: &CallRef) -> Result<(), JsValue> {
    let screen = call.borrow_mut().screen.take();
    if screen.is_some() {
        stop_stream(&screen);
    } else {
        let devices = window().navigator().media_devices()?;
        let options = JSON::parse(r#"{"video":true,"audio":false}"#)?;
        let stream: MediaStream =
            crate::dom::await_promise(js_call(&devices, "getDisplayMedia", &[options]))
                .await?
                .unchecked_into();
        // Le partage arrêté depuis le navigateur.
        if let Ok(track) = stream
            .get_video_tracks()
            .get(0)
            .dyn_into::<MediaStreamTrack>()
        {
            let weak = Rc::downgrade(call);
            let ended = Closure::<dyn FnMut(JsValue)>::new(move |_| {
                if let Some(c) = weak.upgrade() {
                    crate::dom::spawn(async move {
                        let _ = toggle_screen(&c).await;
                    });
                }
            });
            track.set_onended(Some(ended.as_ref().unchecked_ref()));
            ended.forget();
        }
        call.borrow_mut().screen = Some(stream);
    }
    refresh_video(call).await;
    Ok(())
}

pub fn stop(call: &CallRef) {
    let ids: Vec<String> = call.borrow().peers.keys().cloned().collect();
    for id in ids {
        if let Some(p) = call.borrow_mut().peers.remove(&id) {
            p.pc.close();
        }
    }
    let mut c = call.borrow_mut();
    stop_stream(&c.local);
    stop_stream(&c.camera);
    stop_stream(&c.screen);
    c.local = None;
    c.camera = None;
    c.screen = None;
    c.levels.clear();
    if let Some(ctx) = c.audio.take() {
        let _ = ctx.close();
    }
}

/// L'état d'une liaison : `None` connectée, sinon un mot pour l'afficher.
pub fn link_state(call: &Call, id: &str) -> Option<&'static str> {
    let p = call.peers.get(id)?;
    match p.pc.connection_state() {
        RtcPeerConnectionState::Connected => None,
        RtcPeerConnectionState::Failed => Some(crate::i18n::t().link_failed),
        _ => Some(crate::i18n::t().link_connecting),
    }
}

/// Le flux vidéo d'un participant, s'il en envoie un.
pub fn remote_video(call: &Call, id: &str) -> Option<MediaStream> {
    let p = call.peers.get(id)?;
    let live = p.stream.get_video_tracks().iter().any(|t| {
        let t: MediaStreamTrack = t.unchecked_into();
        get(&t, "readyState").as_string().as_deref() == Some("live") && !t.muted()
    });
    // `MediaStream::clone` de web-sys copierait le flux : on garde le même.
    live.then(|| Clone::clone(&p.stream))
}
