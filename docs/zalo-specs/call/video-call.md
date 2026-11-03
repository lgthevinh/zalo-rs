# Video call

Zalo-side facts specific to video calls and the evidence trail behind them: what the official engine reads and does, and what each live capture (2026-10-30) proved. The resulting wire rules are stated once in [media-transport.md](media-transport.md) and [signaling.md](signaling.md); this doc does not repeat them in full.

Evidence tags: `[O]` observed live, or read verbatim from Zalo's JS bundle / binary strings; `[I]` derived from disassembly or inferred, no live confirmation; `[U]` unknown.

## 1. Artifacts examined

| Artifact | Version | Note |
|---|---|---|
| `plugins\capture\ZaloCall.exe` | 26.10.11 | MD5 `A2B920C00115BA11A7FCDCB642147E9C`, 19,587,624 B |
| `plugins\capture\ZaloCall.exe` | 26.10.20 | MD5 `AAA59942E5D8B6BCDE09A8C9E29F26CD`, 19,588,136 B (PE32 x86), no PDB; names come from log strings |
| Renderer JS (`compact-app-pc.*.js`) | 26.10.11 | Signal dispatcher and endpoint table |
| Android `libzrtc.so` (ARM64, from the 26.10.02 APK) | 26.10.02 | Media engine; used to confirm PT and relay type mapping |

Native findings below come from 26.10.20, JS findings from 26.10.11. Whether the video path differs between the two `ZaloCall.exe` builds was not checked. [U]

## 2. Signaling facts (renderer) [O]

- `12006` → `sendOnOffCamMic(calleeId, callId, isOnCam, isOnMic, data)` → `/api/voicecall/conf`, body `{calleeId, callId, isOnCam, isOnMic, data}`, session-AES `params`.
- `11310` is `unholdresp` (signal 414). An older reading that mapped `12006 → requestaudio2video (11310)` was wrong.
- `/api/voicecall/requestaudio2video` is registered as `W(12056, ...)`, but the signal dispatcher (`switch(t){case 401: ...}`) has no `case 12056`.
- Full signal → endpoint table: [signaling.md](signaling.md) §3.

## 3. `_parseExtendData` (`callcontroller.cpp`, string xrefs `0x8d89d3..0x8d9370`) [O]

- Keys read: `callType`, `video` → `codec`, `newZrtc`, `packetMode` (default 2), `gccMode`, `gccAudio`, `gccEarlyCall`, `gccSVLR`, `srtpMode`, `srtcp`, `sP2P`, `maxFT`, `platform`, `fecTP`, `supportHevcDecode`, `tpType`, `spTcp`, `serverAddr`, `select2side`, `extraInfo`, `serverResult`. Log: `_parseExtendData srtpMode = %d`.
- `numServers` exists as a string in the binary but was not seen inside this function.
- Codec entry keys: `name`, `payload`, `frmPtime`, `dynamicFptime`.
- HEVC: without `supportHevcDecode` the engine logs `partner using old version, not support hevc decode`; with it, `partner support hevc decode = %d`. In `libzrtc.so`, a non-zero value (`Peer::_handlePartnerHevcDecCapacityChange`) leads to `_changeVidCodec(5 /* H265 */)` when hardware HEVC encode is available. [I] So a client without an HEVC decoder must send `supportHevcDecode: 0`.

## 4. Audio → video upgrade in the engine [O]

`peercallzrtpimpl.cpp`, `switchToVideoCall`; same call, no new call id. Log-string order: `Begin switchToVideoCall(): startCamera=` → `Call to switchToVideoCall() while not in call !!!` → `You already in a Video Call !!!` → `Invalid zrtc config to switch to video call !!!` → `_audioDevice reinit ...` → `Failed to inti video coding & capture, ret=`. Other strings: `switchToVideoCallAnswer`, `startCamera`, `acceptStartCamera`, `SwitchToVideo`, `updateLocalCameraState`, `updatePartnerCameraState`, `onCallVideoState`, JSON field `video.cameraState`.

`switchToVideoCall` + `acceptStartCamera` suggest a two-sided handshake; the carrier is probably `12006 conf` with `isOnCam=1`. [I, unconfirmed]

## 5. Media engine strings [O]

`zrtc_core` is a WebRTC fork: H.264 (OpenH264, x264, OpenH264SVC), H.265 (FFmpeg, x265, `hevc_mediacodec`), VP8; `RtpPacketizerH264` with FU-A and STAP-A; NACK, PLI (`Incoming PLI from SSRC`), FIR, REMB, transport feedback, RTX, ULPFEC/RED, `abs-send-time`; keyframe settings `keyFrameIntervalMs`, `requestKeyFrameRateMs`, `RequestKeyFrame`, stats `vidKeyFrameEncode/Decode`; `Use ZRTP packetization mode` (xref `0x8c83c6`), `Create VideoRtpRtcp: _callType=` (`0x8c4187`).

## 6. Live captures (2026-10-30)

All captures: 1-1 calls from a desktop test client to an official Zalo peer (mobile or desktop), through a relay `<relay-ip>:4200` (a different relay each call), logged packet-by-packet by the test client. SSRCs and ids are omitted.

### Capture 1 — outgoing video call, peer camera on ~1–2 s, 28 s

Proved: [O]
- Peer advertises `video.codec=[{h264, payload:97}]` but sends RTP on **PT 97 and PT 98**; PT 98 is advertised nowhere.
- Video shares the audio SSRC with its own sequence space; 90 kHz clock, ~30 fps (ts step 2970..3150).
- Payload format is **not RFC 6184**: PT 97 = one whole Annex-B access unit (`00 00 00 01 21 …`), marker 1; PT 98 = `1C <flags> <Annex-B chunk>` with flags `80`/`00`/`40`; first chunk began with SPS (`00 00 00 01 27 42 …`); 3 fragments of 853/853/851 B for the first frame.
- Largest datagram received 874 B = 1 (relay) + 12 (RTP) + 8 (extension) + 853. Biggest single-packet P frame 786 B.
- PLI is real: 56 in 28 s, all on the RTCP socket, outer byte `0x0F`, compound `SR + SDES + PSFB/PLI`, every ~506 ms (487–520), always for our video SSRC. No FIR. 14 NACKs, all for our audio SSRC.
- Relay control datagrams `01 01 …` (47 B) and `02 01 …` (25 B) ~1/s on both sockets.
- Peer `extendData`: `supportHevcDecode:4`, `packetMode:2`, `srtpMode:0`, `srtcp:0`, `negoVidQual:1`, `maxBitratePC:800`, `minBitratePC:150`.
- `onoff_camera` `type:0` stopped the peer's video right away (so `type:1` = on, inferred).

### Capture 2 — peer camera on, PC showed no picture, peer showed "poor network"

Proved: [O]
- The peer did send video: 64 × PT 97 (12.8 pps, ~78 kbps) + 2 × PT 98 by 10 s, `lost=0`; first video packet 5 s after media start.
- Fragmentation: first keyframe (`annexb[7,8,6,5]`) on **PT 98** in 2 fragments of 1083 B (`80`/`40`); a P frame on **PT 97** in 3 fragments of 1062 B (`80`/`00`/`40`), marker on the last only; largest unfragmented payload 1212 B (wire 1233). So the 853 B of capture 1 was path-specific, and AUs are split **evenly**.
- RTP header extension, one-byte form: `be de 00 01 | 51 XX XX 00` (ID 5, 2 bytes, counting up; audio `0001, 0002, …`, video `0109, 010d, 010f, 0112, …`).
- PLI: 25 in 12 s on the RTCP socket, ~500 ms apart, starting 0.8 s after media start. NACK: 10, audio only.
- An earlier call in the same session with the **peer camera off** still received PLI from ~27 s in (22 PLI, ~500 ms apart), this time addressed to the peer's own SSRC. Unexplained. [U]

### Capture 3 — peer camera on, phone said "PC connection unstable"

Proved: [O]
- The peer sent **only P frames** for the whole call (no SPS/PPS/IDR, no PT 98) and the test client sent no PLI: without a keyframe nothing is decodable (`non-existing PPS 0 referenced`, 0 frames). The peer does not volunteer a keyframe; the receiver must ask.
- The peer's report blocks about our streams showed ~78% loss on both audio and video, in bursts hitting both at once (`fraction` 250–253 for a few seconds, then 0); highest-seq advanced at ~47 pps for audio.
- The peer's report blocks had `LSR=0`, `DLSR=0` throughout: it never received our Sender Reports as RTCP.
- PLI from the peer every ~500 ms all call (~60). 40 relay control datagrams `01 01 …` at 1/s (47 B, 29 B twice at start).

### Capture 4 — peer camera briefly on

Proved: [O]
- Local UDP sends all succeeded (no `WouldBlock`/errors, no loop stall), and the peer reported ~0% loss (audio 2/503, video 0/181). This does not prove what caused the ~78% loss of capture 3 (a different call and relay); the socket-usage issue found in capture 5 is a candidate. [U]
- **Our PLI was answered immediately with a keyframe** (PT 98, `zfrag/S/annexb[7,8,6,5]`, 961 + 961 B, AU 1918 B), followed by P frames.
- The peer sent only 22 video packets in ~0.55 s before `onoff_camera type:0`. A receiver must be able to show a picture from a very short burst (decoder latency matters; a pipe-based decoder that needs ~30 AUs to start showed nothing).

### Capture 5 — single-port relay, the RTCP socket swallowed media

Proved: [O]
- The test client's own reception stats showed ~71% of the peer's audio "missing" and the video keyframe never reaching its decoder, while a packet log of **both** sockets showed every peer packet arriving (`lost=0`: audio 477, video 2 × PT 98 keyframe + 59 × PT 97). The missing packets had arrived on the RTCP socket, which only handled RTCP.
- Peer SRs referenced bogus SSRCs (`0x81ce0002`, NTP-derived values): our RTCP had been delivered to it as video RTP.

Root causes, from the code that produced the capture plus `libzrtc.so` disassembly: [I]
1. On a single-port relay (RTP == RTCP address), init/ping packets sent from the RTCP socket after the RTP socket made the relay deliver media to the RTCP socket's port. Rule: accept RTP on both sockets; do not ping from the RTCP socket when the addresses are equal.
2. Relay type bytes depend on media kind and RTP-vs-RTCP only (`0x03` audio RTP, `0x05` audio RTCP, `0x0D` video RTP, `0x0F` video RTCP); RTCP sent with `0x0D` is forwarded as video RTP. This also explains `LSR=0` in captures 3 and 5.
3. H.264 keyframes must use PT 98 even when unfragmented: `libzrtc.so` maps H.264 key ↔ PT 98, delta ↔ PT 97 (H.265 121/122, SVC 126/125) and forces any PT 97 packet to "delta frame".
4. RTP header extension ID 5 = transport-wide sequence number (TWCC).
5. The `01 01` / `02 01` datagrams are `ZRTPPacket` `cmd 0x0020 RequestForward`, `subCmd 0x0A VideoControl` / `0x14 IncallNegotiateInfo`.

The fixes for 1–3 were never tried on a live call. [I]

## 7. Conclusions for implementers

- Use the Annex-B access-unit packetisation and PT 97/98 rules of [media-transport.md](media-transport.md) §5; do not send RFC 6184.
- Send PLI until the first SPS/IDR is decoded and after gaps; answer PLI/FIR with a full keyframe on PT 98.
- Read both sockets for both RTP and RTCP; use the correct relay type byte for RTCP.
- Advertise `supportHevcDecode: 0` unless HEVC decode exists.
- Send a real encoded picture: a synthetic/invalid IDR makes the peer request keyframes at 2 Hz forever.

## Open questions

- [U] Whether the peer decodes our video once all transport rules above are applied (no capture after the fixes).
- [U] Whether the missing TWCC extension/feedback is what the peer reports as "poor network".
- [U] Meaning of the video stream's extension values and of `supportHevcDecode:4` vs `1`.
- [U] Why PLI begins late, and targets the peer's own SSRC, in a camera-off call.
- [U] Whether the relay accepts 1200-byte payloads from a client.
- [U] Signaling carrier for the audio → video upgrade, content of `conf.data`, and who calls `requestaudio2video` (12056).
- [U] Whether 26.10.20 changes any of the above vs 26.10.11.
- [U] Contradiction: capture 4's PLI got an immediate keyframe, yet the capture-5 analysis says RTCP was being sent with the wrong relay type (`0x0D`) until then; which type that PLI actually used is not recorded unambiguously.
