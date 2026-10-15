// Les appels audio et vidéo : WebRTC, de pair à pair, avec chaque membre de
// l'appel. Le même protocole que le navigateur (`client/src/call.rs`) :
//
// - qui est dans l'appel, micro coupé ou caméra : annoncé par `hello` ;
// - entre deux membres, celui dont l'identifiant est le plus petit fait
//   l'offre : un transceiver audio (son micro) et un vidéo (sa caméra,
//   ou rien) ;
// - l'offre, la réponse et les candidats ICE passent par le salon,
//   chiffrés et signés pour ce seul membre (`signal`) ;
// - une connexion qui échoue est relancée (redémarrage ICE) par l'offrant.
//
// Le son et l'image vont directement d'un appareil à l'autre (DTLS-SRTP) :
// le serveur n'en voit rien.

import AVFoundation
import AlleyKit
import Foundation
import Observation
import WebRTC

@MainActor
@Observable
final class CallEngine {
    /// Une connexion avec un membre de l'appel.
    final class Link: NSObject, RTCPeerConnectionDelegate {
        let id: String
        let offerer: Bool
        var pc: RTCPeerConnection!
        var videoSender: RTCRtpSender?
        var pending: [RTCIceCandidate] = []
        weak var engine: CallEngine?

        init(id: String, offerer: Bool, engine: CallEngine) {
            self.id = id
            self.offerer = offerer
            self.engine = engine
        }

        // Les rappels de WebRTC arrivent sur son propre fil : on revient au
        // fil principal.
        func peerConnection(_ pc: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {
            let json: [String: Any] = [
                "candidate": candidate.sdp, "sdpMid": candidate.sdpMid ?? "",
                "sdpMLineIndex": Int(candidate.sdpMLineIndex),
            ]
            let id = id
            Task { @MainActor [weak engine] in engine?.room.signal(to: id, ["ice": json]) }
        }

        func peerConnection(_ pc: RTCPeerConnection, didAdd rtpReceiver: RTCRtpReceiver, streams: [RTCMediaStream]) {
            guard let video = rtpReceiver.track as? RTCVideoTrack else { return }
            let id = id
            Task { @MainActor [weak engine] in engine?.remoteVideo[id] = video }
        }

        func peerConnection(_ pc: RTCPeerConnection, didChange newState: RTCPeerConnectionState) {
            let (id, offerer) = (id, offerer)
            Task { @MainActor [weak engine] in
                guard let engine else { return }
                engine.states[id] = newState
                if newState == .failed && offerer { engine.offer(to: id, restart: true) }
            }
        }

        func peerConnection(_ pc: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
        func peerConnection(_ pc: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
        func peerConnection(_ pc: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
        func peerConnectionShouldNegotiate(_ pc: RTCPeerConnection) {}
        func peerConnection(_ pc: RTCPeerConnection, didChange newState: RTCIceConnectionState) {}
        func peerConnection(_ pc: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
        func peerConnection(_ pc: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
        func peerConnection(_ pc: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}
    }

    static let factory: RTCPeerConnectionFactory = {
        RTCInitializeSSL()
        return RTCPeerConnectionFactory(
            encoderFactory: RTCDefaultVideoEncoderFactory(), decoderFactory: RTCDefaultVideoDecoderFactory())
    }()

    let room: RoomSession
    let iceServers: [RTCIceServer]
    @ObservationIgnored var links: [String: Link] = [:]
    @ObservationIgnored var audioTrack: RTCAudioTrack?
    @ObservationIgnored var capturer: RTCCameraVideoCapturer?
    @ObservationIgnored var frontCamera = true

    /// Ma caméra (pour l'aperçu), et celles des autres.
    var localVideo: RTCVideoTrack?
    var remoteVideo: [String: RTCVideoTrack] = [:]
    var states: [String: RTCPeerConnectionState] = [:]
    var muted = false
    var speaker = true

    init(room: RoomSession, iceServers: [[String: Any]]) {
        self.room = room
        self.iceServers = iceServers.compactMap { s in
            let urls = (s["urls"] as? [String]) ?? (s["urls"] as? String).map { [$0] } ?? []
            guard !urls.isEmpty else { return nil }
            return RTCIceServer(urlStrings: urls, username: s["username"] as? String, credential: s["credential"] as? String)
        }
    }

    var camera: Bool { localVideo != nil }

    // MARK: Entrer, sortir

    /// Entre dans l'appel : le micro (avec la permission), puis les
    /// connexions avec ceux qui y sont déjà. Faux sans micro.
    func start() async -> Bool {
        guard await AVAudioApplication.requestRecordPermission() else { return false }
        configureAudio()
        let source = Self.factory.audioSource(with: RTCMediaConstraints(
            mandatoryConstraints: nil,
            optionalConstraints: ["googEchoCancellation": "true", "googNoiseSuppression": "true", "googAutoGainControl": "true"]))
        audioTrack = Self.factory.audioTrack(with: source, trackId: "alley-audio")
        room.onSignal = { [weak self] from, data in self?.receive(from: from, data) }
        room.onCallChanged = { [weak self] in self?.sync() }
        room.setCall(inCall: true)
        sync()
        return true
    }

    func stop() {
        room.onSignal = nil
        room.onCallChanged = nil
        for id in Array(links.keys) { drop(id) }
        capturer?.stopCapture()
        capturer = nil
        localVideo = nil
        audioTrack = nil
        remoteVideo.removeAll()
        if room.state == .ready { room.setCall(inCall: false) }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func configureAudio() {
        let session = RTCAudioSession.sharedInstance()
        session.lockForConfiguration()
        try? session.setCategory(.playAndRecord, with: [.defaultToSpeaker, .allowBluetooth, .allowBluetoothA2DP])
        try? session.setMode(.videoChat)
        session.unlockForConfiguration()
    }

    /// Relie l'appel à ceux qui y sont (et lâche ceux qui en sont partis).
    func sync() {
        guard room.inCall else { return stop() }
        let members = Set(room.callMembers).subtracting([room.me])
        for id in members where links[id] == nil { connect(id) }
        for id in links.keys where !members.contains(id) { drop(id) }
    }

    func drop(_ id: String) {
        links.removeValue(forKey: id)?.pc.close()
        remoteVideo[id] = nil
        states[id] = nil
    }

    // MARK: Les connexions

    func connect(_ id: String) {
        let config = RTCConfiguration()
        config.iceServers = iceServers
        config.sdpSemantics = .unifiedPlan
        config.continualGatheringPolicy = .gatherContinually
        let link = Link(id: id, offerer: room.me < id, engine: self)
        guard let pc = Self.factory.peerConnection(
            with: config, constraints: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil),
            delegate: link)
        else { return }
        link.pc = pc
        links[id] = link
        if link.offerer {
            if let audioTrack {
                _ = pc.add(audioTrack, streamIds: ["alley"])
            } else {
                let init_ = RTCRtpTransceiverInit()
                init_.direction = .recvOnly
                _ = pc.addTransceiver(of: .audio, init: init_)
            }
            let init_ = RTCRtpTransceiverInit()
            init_.direction = .sendRecv
            let video = pc.addTransceiver(of: .video, init: init_)
            video?.sender.track = localVideo
            link.videoSender = video?.sender
            offer(to: id, restart: false)
        }
    }

    func offer(to id: String, restart: Bool) {
        guard let link = links[id] else { return }
        let constraints = RTCMediaConstraints(
            mandatoryConstraints: restart ? ["IceRestart": "true"] : nil, optionalConstraints: nil)
        link.pc.offer(for: constraints) { [weak self] sdp, _ in
            guard let sdp else { return }
            link.pc.setLocalDescription(sdp) { error in
                guard error == nil else { return }
                Task { @MainActor in self?.room.signal(to: id, ["sdp": Self.json(sdp)]) }
            }
        }
    }

    static func json(_ sdp: RTCSessionDescription) -> [String: Any] {
        ["type": RTCSessionDescription.string(for: sdp.type), "sdp": sdp.sdp]
    }

    /// Une offre, une réponse ou un candidat ICE d'un membre.
    func receive(from: String, _ data: [String: Any]) {
        if links[from] == nil {
            guard room.callMembers.contains(from) || data["sdp"] != nil else { return }
            connect(from)
        }
        guard let link = links[from] else { return }
        if let sdp = data["sdp"] as? [String: Any], let type = sdp["type"] as? String, let text = sdp["sdp"] as? String {
            let desc = RTCSessionDescription(type: RTCSessionDescription.type(for: type), sdp: text)
            if type == "offer" && !link.offerer {
                link.pc.setRemoteDescription(desc) { [weak self] error in
                    guard error == nil else { return }
                    Task { @MainActor in self?.answer(link) }
                }
            } else if type == "answer" && link.offerer && link.pc.signalingState == .haveLocalOffer {
                link.pc.setRemoteDescription(desc) { [weak self] error in
                    guard error == nil else { return }
                    Task { @MainActor in self?.flush(link) }
                }
            }
        } else if let ice = data["ice"] as? [String: Any], let candidate = ice["candidate"] as? String {
            let c = RTCIceCandidate(
                sdp: candidate, sdpMLineIndex: Int32((ice["sdpMLineIndex"] as? NSNumber)?.intValue ?? 0),
                sdpMid: ice["sdpMid"] as? String)
            if link.pc.remoteDescription != nil {
                link.pc.add(c) { _ in }
            } else {
                link.pending.append(c)
            }
        }
    }

    /// Répondre à une offre : mon micro et ma caméra sur ses transceivers.
    func answer(_ link: Link) {
        for t in link.pc.transceivers {
            t.setDirection(.sendRecv, error: nil)
            if t.mediaType == .audio {
                t.sender.track = audioTrack
            } else if t.mediaType == .video {
                t.sender.track = localVideo
                link.videoSender = t.sender
            }
        }
        let id = link.id
        link.pc.answer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)) { [weak self] sdp, _ in
            guard let sdp else { return }
            link.pc.setLocalDescription(sdp) { error in
                guard error == nil else { return }
                Task { @MainActor in
                    self?.room.signal(to: id, ["sdp": Self.json(sdp)])
                    self?.flush(link)
                }
            }
        }
    }

    func flush(_ link: Link) {
        let pending = link.pending
        link.pending.removeAll()
        for c in pending { link.pc.add(c) { _ in } }
    }

    // MARK: Micro, caméra, haut-parleur

    func toggleMute() {
        muted.toggle()
        audioTrack?.isEnabled = !muted
        room.setCall(inCall: true, muted: muted, video: camera)
    }

    func toggleSpeaker() {
        speaker.toggle()
        let session = RTCAudioSession.sharedInstance()
        session.lockForConfiguration()
        try? session.overrideOutputAudioPort(speaker ? .speaker : .none)
        session.unlockForConfiguration()
    }

    /// La caméra (avant par défaut) : partagée avec tous les membres.
    func toggleCamera() async {
        if camera {
            capturer?.stopCapture()
            capturer = nil
            localVideo = nil
        } else {
            guard await AVCaptureDevice.requestAccess(for: .video) else { return }
            let source = Self.factory.videoSource()
            let capturer = RTCCameraVideoCapturer(delegate: source)
            self.capturer = capturer
            localVideo = Self.factory.videoTrack(with: source, trackId: "alley-video")
            startCapture()
        }
        for link in links.values { link.videoSender?.track = localVideo }
        room.setCall(inCall: true, muted: muted, video: camera)
    }

    func switchCamera() {
        frontCamera.toggle()
        startCapture()
    }

    func startCapture() {
        guard let capturer else { return }
        let position: AVCaptureDevice.Position = frontCamera ? .front : .back
        let devices = RTCCameraVideoCapturer.captureDevices()
        guard let device = devices.first(where: { $0.position == position }) ?? devices.first else { return }
        // Le format le plus proche de 1280 × 720.
        let formats = RTCCameraVideoCapturer.supportedFormats(for: device)
        let format = formats.min { a, b in
            let da = CMVideoFormatDescriptionGetDimensions(a.formatDescription)
            let db = CMVideoFormatDescriptionGetDimensions(b.formatDescription)
            return abs(Int(da.width) - 1280) + abs(Int(da.height) - 720) < abs(Int(db.width) - 1280) + abs(Int(db.height) - 720)
        }
        guard let format else { return }
        let fps = min(30, Int(format.videoSupportedFrameRateRanges.map(\.maxFrameRate).max() ?? 30))
        capturer.startCapture(with: device, format: format, fps: fps)
    }
}
