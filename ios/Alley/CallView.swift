// L'appel, en haut du salon : qui y est (vidéo ou initiales, micro coupé),
// mon aperçu, et les commandes (micro, caméra, haut-parleur, raccrocher).
// Hors de l'appel : un bandeau pour le rejoindre.

import AlleyKit
import SwiftUI
import UIKit
import WebRTC

struct CallPanel: View {
    @Environment(AppModel.self) private var model
    let room: RoomSession

    var body: some View {
        let t = model.t
        if let call = model.call {
            VStack(spacing: 10) {
                let others = room.callMembers.filter { $0 != room.me }
                if others.isEmpty {
                    Text(t.noCallMembers).font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(others, id: \.self) { id in
                            Tile(
                                name: room.name(of: id), id: id, track: call.remoteVideo[id],
                                muted: room.peers[id]?.muted == true, mirror: false,
                                connecting: call.states[id] != .connected)
                        }
                        Tile(name: "\(model.pseudonym) (\(t.you))", id: "moi", track: call.localVideo, muted: call.muted, mirror: call.frontCamera, connecting: false)
                    }
                    .padding(.horizontal, 12)
                }
                HStack(spacing: 14) {
                    Control(icon: call.muted ? "mic.slash.fill" : "mic.fill", label: call.muted ? t.unmuteMic : t.muteMic, active: call.muted) {
                        call.toggleMute()
                    }
                    Control(icon: call.camera ? "video.fill" : "video.slash.fill", label: call.camera ? t.cameraOff : t.cameraOn, active: call.camera) {
                        Task { await call.toggleCamera() }
                    }
                    if call.camera {
                        Control(icon: "arrow.triangle.2.circlepath.camera", label: t.switchCamera, active: false) { call.switchCamera() }
                    }
                    Control(icon: call.speaker ? "speaker.wave.3.fill" : "speaker.fill", label: t.speakerLabel, active: call.speaker) {
                        call.toggleSpeaker()
                    }
                    Button {
                        model.leaveCall()
                    } label: {
                        Image(systemName: "phone.down.fill")
                            .font(.title3)
                            .foregroundStyle(.white)
                            .frame(width: 52, height: 52)
                            .background(Color.red, in: .circle)
                    }
                    .accessibilityLabel(t.leaveCall)
                    .accessibilityIdentifier("hangup")
                }
            }
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .background(Color(.secondarySystemBackground))
        } else if !room.callMembers.isEmpty {
            Button {
                Task { await model.joinCall() }
            } label: {
                HStack {
                    Image(systemName: "phone.fill")
                    Text("\(t.callOngoing) · \(room.callMembers.map { room.name(of: $0) }.joined(separator: ", "))")
                        .lineLimit(1)
                    Spacer()
                    Text(t.joinCall).fontWeight(.semibold)
                }
                .font(.subheadline)
                .foregroundStyle(.white)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color.alleyMint)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("join-call")
        }
    }

    struct Control: View {
        let icon: String
        let label: String
        let active: Bool
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: icon)
                    .font(.title3)
                    .foregroundStyle(active ? Color.black : Color.primary)
                    .frame(width: 52, height: 52)
                    .background(active ? Color.white : Color(.tertiarySystemFill), in: .circle)
            }
            .accessibilityLabel(label)
        }
    }

    /// Une personne de l'appel : sa vidéo, ou ses initiales.
    struct Tile: View {
        let name: String
        let id: String
        let track: RTCVideoTrack?
        let muted: Bool
        let mirror: Bool
        let connecting: Bool

        var body: some View {
            ZStack(alignment: .bottomLeading) {
                if let track {
                    VideoView(track: track, mirror: mirror)
                } else {
                    Color(.tertiarySystemBackground)
                    Avatar(id: id, name: name, size: 56)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                HStack(spacing: 4) {
                    if muted { Image(systemName: "mic.slash.fill").foregroundStyle(.red) }
                    Text(name).lineLimit(1)
                    if connecting { ProgressView().scaleEffect(0.6) }
                }
                .font(.caption2.weight(.semibold))
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(.ultraThinMaterial, in: .capsule)
                .padding(6)
            }
            .frame(width: 150, height: 200)
            .clipShape(.rect(cornerRadius: 14))
            .accessibilityElement(children: .combine)
        }
    }
}

/// Une vidéo WebRTC (Metal).
struct VideoView: UIViewRepresentable {
    let track: RTCVideoTrack
    let mirror: Bool

    final class Coordinator {
        var track: RTCVideoTrack?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> RTCMTLVideoView {
        let view = RTCMTLVideoView(frame: .zero)
        view.videoContentMode = .scaleAspectFill
        view.clipsToBounds = true
        return view
    }

    func updateUIView(_ view: RTCMTLVideoView, context: Context) {
        view.transform = mirror ? CGAffineTransform(scaleX: -1, y: 1) : .identity
        if context.coordinator.track !== track {
            context.coordinator.track?.remove(view)
            track.add(view)
            context.coordinator.track = track
        }
    }

    static func dismantleUIView(_ view: RTCMTLVideoView, coordinator: Coordinator) {
        coordinator.track?.remove(view)
    }
}
