// Les appels audio et vidéo d'Alley : WebRTC de pair à pair (maillage, un
// lien par participant), média chiffré par DTLS-SRTP.
//
// La mise en relation (offres, réponses, candidats ICE) passe par le relais
// chiffré et signé du serveur Alley : le serveur ne peut ni lire ni modifier
// les empreintes DTLS, donc ni écouter ni s'intercaler. Un TURN éventuel ne
// relaie que du chiffré.
//
// Pour éviter les offres croisées, c'est toujours le participant à
// l'identifiant le plus petit qui appelle l'autre.

export class Call {
  /**
   * @param {object} o
   * @param {string} o.me           mon identifiant de membre
   * @param {string} o.channel      le salon vocal
   * @param {RTCIceServer[]} o.iceServers
   * @param {(to: string, data: object) => void} o.signal  envoie un signal chiffré
   * @param {() => void} o.onChange appelé quand l'appel change (flux, micro…)
   */
  constructor({ me, channel, iceServers, signal, onChange }) {
    this.me = me;
    this.channel = channel;
    this.iceServers = iceServers;
    this.signal = signal;
    this.onChange = onChange;
    this.peers = new Map(); // membre → { pc, offerer, stream, video }
    this.local = null;
    this.camera = null;
    this.screen = null;
    this.muted = false;
    this.levels = new Map();
    this.audio = null;
  }

  /** Ouvre le micro (le geste de l'utilisateur est requis sur iOS). */
  async start() {
    this.local = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      video: false,
    });
    this.watchLevel(this.me, this.local);
    this.onChange();
  }

  /** Les participants annoncés dans le salon : on se relie à chacun. */
  sync(participants) {
    for (const id of participants) if (id !== this.me && !this.peers.has(id)) this.connect(id);
    for (const id of [...this.peers.keys()]) if (!participants.includes(id)) this.drop(id);
  }

  /**
   * Relie à un participant. Celui des deux dont l'identifiant est le plus
   * petit propose l'appel (une ligne audio, une ligne vidéo) ; l'autre répond.
   * Allumer ou couper la caméra ne fait ensuite que changer de piste
   * (`replaceTrack`), sans renégocier.
   */
  connect(id) {
    const pc = new RTCPeerConnection({ iceServers: this.iceServers });
    const peer = { pc, offerer: this.me < id, stream: new MediaStream(), video: null, pending: [] };
    this.peers.set(id, peer);
    pc.onicecandidate = ({ candidate }) => candidate && this.signal(id, { ice: candidate.toJSON() });
    pc.ontrack = ({ track, transceiver }) => {
      if (track.kind === "video") peer.video = transceiver.sender;
      peer.stream.addTrack(track);
      for (const e of ["ended", "mute", "unmute"]) track.addEventListener(e, () => this.onChange());
      if (track.kind === "audio") this.watchLevel(id, new MediaStream([track]));
      this.onChange();
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === "failed" && peer.offerer) this.offer(id, peer, true);
      this.onChange();
    };
    if (peer.offerer) {
      const audio = this.local?.getAudioTracks()[0];
      if (audio) pc.addTrack(audio, this.local);
      else pc.addTransceiver("audio", { direction: "recvonly" });
      peer.video = pc.addTransceiver("video", { direction: "sendrecv" }).sender;
      const video = this.videoTrack();
      if (video) peer.video.replaceTrack(video);
      this.offer(id, peer);
    }
    this.onChange();
    return peer;
  }

  async offer(id, peer, restart = false) {
    try {
      const offer = await peer.pc.createOffer({ iceRestart: restart });
      await peer.pc.setLocalDescription(offer);
      this.signal(id, { sdp: peer.pc.localDescription.toJSON() });
    } catch (err) {
      console.warn("offre", err);
    }
  }

  /** Un signal reçu (déjà déchiffré et vérifié). */
  async receive(from, data) {
    const peer = this.peers.get(from) || this.connect(from);
    const pc = peer.pc;
    try {
      if (data.sdp?.type === "offer" && !peer.offerer) {
        await pc.setRemoteDescription(data.sdp);
        for (const t of pc.getTransceivers()) {
          t.direction = "sendrecv";
          if (t.receiver.track.kind === "audio") {
            const audio = this.local?.getAudioTracks()[0];
            if (audio) await t.sender.replaceTrack(audio);
          } else {
            peer.video = t.sender;
            await t.sender.replaceTrack(this.videoTrack());
          }
        }
        await pc.setLocalDescription(await pc.createAnswer());
        this.signal(from, { sdp: pc.localDescription.toJSON() });
        await this.flush(peer);
      } else if (data.sdp?.type === "answer" && peer.offerer && pc.signalingState === "have-local-offer") {
        await pc.setRemoteDescription(data.sdp);
        await this.flush(peer);
      } else if (data.ice) {
        // Un candidat peut arriver avant l'offre : on le garde pour après.
        if (pc.remoteDescription) await pc.addIceCandidate(data.ice).catch(() => {});
        else peer.pending.push(data.ice);
      }
    } catch (err) {
      console.warn("signal", err);
    }
  }

  async flush(peer) {
    for (const ice of peer.pending.splice(0)) await peer.pc.addIceCandidate(ice).catch(() => {});
  }

  drop(id) {
    const peer = this.peers.get(id);
    if (!peer) return;
    peer.pc.close();
    this.peers.delete(id);
    this.levels.delete(id);
    this.onChange();
  }

  videoTrack() {
    const video = this.screen || this.camera;
    return video ? video.getVideoTracks()[0] : null;
  }

  /** Change la piste vidéo envoyée à tous (caméra, écran ou rien). */
  async refreshVideo() {
    const track = this.videoTrack();
    await Promise.all([...this.peers.values()].map((p) => p.video?.replaceTrack(track).catch(() => {})));
    this.onChange();
  }

  toggleMute() {
    this.muted = !this.muted;
    for (const t of this.local?.getAudioTracks() || []) t.enabled = !this.muted;
    this.onChange();
  }

  async toggleCamera() {
    if (this.camera) {
      for (const t of this.camera.getTracks()) t.stop();
      this.camera = null;
    } else {
      this.camera = await navigator.mediaDevices.getUserMedia({
        video: { width: { ideal: 1280 }, height: { ideal: 720 }, facingMode: "user" },
      });
    }
    await this.refreshVideo();
  }

  static canShareScreen() {
    return !!navigator.mediaDevices?.getDisplayMedia;
  }

  async toggleScreen() {
    if (this.screen) {
      for (const t of this.screen.getTracks()) t.stop();
      this.screen = null;
    } else {
      this.screen = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: false });
      this.screen.getVideoTracks()[0].onended = () => this.screen && this.toggleScreen();
    }
    await this.refreshVideo();
  }

  /** Qui parle : un niveau sonore par participant, mesuré localement. */
  watchLevel(id, stream) {
    try {
      this.audio ||= new (window.AudioContext || window.webkitAudioContext)();
      const source = this.audio.createMediaStreamSource(stream);
      const analyser = this.audio.createAnalyser();
      analyser.fftSize = 512;
      source.connect(analyser);
      const data = new Uint8Array(analyser.fftSize);
      this.levels.set(id, () => {
        analyser.getByteTimeDomainData(data);
        let sum = 0;
        for (const v of data) sum += (v - 128) ** 2;
        return Math.sqrt(sum / data.length);
      });
    } catch {
      // Sans Web Audio : pas d'indicateur, l'appel marche quand même.
    }
  }

  speaking(id) {
    if (id === this.me && this.muted) return false;
    const level = this.levels.get(id);
    return level ? level() > 6 : false;
  }

  stop() {
    for (const id of [...this.peers.keys()]) this.drop(id);
    for (const s of [this.local, this.camera, this.screen]) for (const t of s?.getTracks() || []) t.stop();
    this.local = this.camera = this.screen = null;
    this.audio?.close().catch(() => {});
    this.audio = null;
  }
}
