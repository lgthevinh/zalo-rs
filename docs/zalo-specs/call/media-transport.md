# Call media transport

Wire format of call media after signaling: the UDP relay framing, RTP/RTCP layout, Opus audio, Zalo's H.264 packetisation (not RFC 6184), RTCP reports and keyframe requests, header extensions, timing, and the group SFU control packets. Signaling and relay registration are in [signaling.md](signaling.md); dated capture results that back these facts are in [video-call.md](video-call.md).

Evidence tags: `[O]` observed in live captures (2026-10-30, 1-1 calls through a relay on UDP port 4200, peer = official Zalo mobile/desktop), or read verbatim from Zalo binaries' strings; `[I]` implemented in the previous client, or derived from disassembly of `ZaloCall.exe` / Android `libzrtc.so`, without live confirmation; `[U]` unknown.

## 1. Sockets and addressing

- Each leg uses two local UDP sockets, "RTP" and "RTCP", registered with the relay (see [signaling.md](signaling.md) §8). Media goes to the relay `rtpAddress`, RTCP to `rtcpAddress`. On 1-1 relays both are the same `<relay-ip>:4200`. [O]
- Incoming RTP and RTCP can arrive on **either** socket. Every observed peer PLI arrived on the RTCP socket (56/56 in one call, 25/25 in another). [O] In one call most incoming RTP also landed on the RTCP socket after pings were sent from it to a single-port relay. A receiver must demultiplex both sockets by content. [O for the observation; cause `[I]`]
- RTP vs RTCP on the same socket: inner byte 1 in `200..=207` = RTCP (RFC 5761 rule). [I]
- No SRTP: RTP and RTCP from the peer were readable plaintext with `srtpMode:0`, `srtcp:0`. [O]

## 2. Relay framing

Every client→relay media datagram is `type (u8) | token (u32 LE) | RTP-or-RTCP packet`, where `token` is the value the relay returned in its InitZRTP reply. [O] (media sent this way reached the peer.)

| Type byte | Direction | Header | Content |
|---|---|---|---|
| `0x03` | client → relay | 5 B (type + token) | Audio RTP |
| `0x05` | client → relay, relay → client | 5 B | Audio RTCP |
| `0x0D` | client → relay | 5 B | Video RTP |
| `0x0F` | client → relay, relay → client | 5 B | Video RTCP |
| `0x04` | relay → client | 1 B | Audio RTP |
| `0x0E` | relay → client | 1 B | Video RTP |
| `0x07` | relay → client | 1 B | P2P RTP `[I]` |
| `0x7F` | relay → client | 9 B: `7F 00 role callId(u32 LE) cmd(u16 LE)`, media when `cmd ∈ 6..=10` | `[I]` |
| `0x02` + cmd `0x0032` | relay → client | 25 B | Media behind a ZRTP response header `[I]` |
| first byte `0x80..0xBF` | either | none | Raw RTP v2 (P2P / no token) `[I]` |

- Incoming `0x04` / `0x0E` with a 1-byte header: `[O]` (largest received datagram 874 B = 1 + 12 RTP + 8 ext + 853 payload).
- Incoming RTCP with outer `0x0F` (video) and `0x05` (audio), 5-byte header: `[O]` (from a few samples; mapping per `libzrtc.so` `[I]`).
- The type byte depends only on media kind and RTP-vs-RTCP, **never on caller/callee role** (`ZRTPPacket::initZRTPPacketAudio` / `initZRTPPacketVideo`, `_buildPacketInternal` bitmask `0xa0a8` in `libzrtc.so`). [I] Sending RTCP with `0x0D` makes the relay forward it to the peer as video RTP (`0x0E`): the peer then reads RTCP bytes as RTP SSRCs (bogus SSRCs `0x81ce0002`, NTP-derived values observed in its reports) and never sees the RTCP. [O for the symptom, `[I]` for the cause]
- Unknown relay datagrams seen ~1/s on both sockets: `01 01 ...` (47 B, 29 B at call start) and `02 01 ...` (25 B), carrying a counter. Per `libzrtc.so` they are ZRTP control requests/responses `cmd 0x0020 RequestForward` with `subCmd 0x0A VideoControl` or `0x14 IncallNegotiateInfo`, layout = 21-byte common header + `len u32 LE` + payload. [O for presence/sizes, `[I]` for meaning]

## 3. RTP

- RTP v2, 12-byte header, big-endian (RFC 3550). [O]
- The peer used **one SSRC for audio and video**, with separate sequence-number spaces. [O] The peer's SSRC looked like a 32-bit user id. `[U]` whether that is required; the previous client used `ssrc = own 32-bit id` for audio and `+1` for video, and the peer reported on both. [O for the peer reporting on both]
- Every peer RTP packet (audio and video) carried a one-byte header extension (`X=1`): `be de 00 01 | 51 XX XX 00` = ID 5, 2 data bytes, 1 pad byte. The 2-byte value counts up per packet (audio `0001, 0002, …`; video interleaved values `0109, 010d, 010f, 0112, …`). [O] ID 5 is `kRtpTransportSequenceNumberHeaderExtensionDefaultId` (transport-wide sequence number / TWCC) in `libzrtc.so`. [I] Receivers must skip the extension (and CSRCs, padding) when locating the payload.
- The previous client sent 1-1 RTP **without** header extension and no TWCC feedback; the peer still received both streams and reported on them in its RTCP. [O] Whether the missing extension/feedback causes the peer's "poor network" / "unstable connection" indicator is `[U]`.

## 4. Audio

- Codec: Opus, payload type **112**, `opus/16000/1` — 16 kHz, mono, 20 ms frames (`frmPtime 20`, `dynamicFptime 0`). [O] for PT 112 on peer packets; parameters from the `codec` string `[I]`.
- 320 samples per frame; RTP timestamp += 320 per packet; ~50 packets/s. [I]
- Keep sending every 20 ms even when the microphone yields nothing (encode silence): a stream that drops to 0 kbps was associated with the peer's "unstable connection" warning. [I]
- The peer sent RTCP NACKs (10–14 per call) for our **audio** SSRC only. Handling them is optional/unknown. [O]

## 5. Video

### 5.1 Codec and clock

- H.264, clock 90 kHz. Peer's `extendData` advertises `video.codec=[{name:"h264", payload:97}]`. [O]
- Peer frame rate seen ~30 fps (timestamp step 2970..3150) in one call and ~12.8 packets/s (~78 kbps) in another. [O]
- Peer stream content: AUs of NAL types `[7,8,6,5]` (SPS, PPS, SEI, IDR) for keyframes and `[1]` for P frames. [O]

### 5.2 Payload types

| PT | Meaning |
|---|---|
| 97 (`0x61`) | H.264 delta frame (`kVideoFrameDelta`) |
| 98 (`0x62`) | H.264 key frame (`kVideoFrameKey`) |
| 121 / 122 | H.265 key / delta |
| 126 / 125 | SVC key / delta |

`libzrtc.so` (`RTPSender::SendOutgoingData`, `RTPReceiverVideo::ParseRtpPacket`) maps frame type ↔ PT unconditionally; a receiver that gets H.264 on PT 97 marks it a delta frame even if it contains SPS+PPS+IDR. So a keyframe must be sent on **PT 98 whether or not it is fragmented**. [I] Captures agree: PT 98 only ever carried the SPS/PPS/SEI/IDR access unit; fragmented P frames used PT 97. [O] PT 98 is not advertised anywhere in signaling. [O]

### 5.3 Packetisation ("annexb-au", not RFC 6184)

- **One access unit per RTP frame**, as an Annex-B byte stream (`00 00 00 01 <NAL> 00 00 00 01 <NAL> …`). Marker bit = 1 on the last packet of the AU only. [O]
- **Fits in one packet:** payload = the whole Annex-B AU (e.g. `00 00 00 01 21 …`). Largest unfragmented payload seen: 1212 B. [O]
- **Too big:** split the Annex-B AU into fragments, each prefixed with 2 bytes `1C <flags>`: [O]
  - `0x1C` = indicator (low 5 bits 28, as FU-A, but the following byte is not an FU header);
  - flags `0x80` first, `0x00` middle, `0x40` last (low 5 bits always 0; a single-fragment AU would be `0xC0` `[I]`);
  - the chunk is a raw slice of the Annex-B stream, so the first fragment starts with `00 00 00 01`.
- **Even split:** the peer cuts an AU into `n = ceil(len / ~1200)` parts of near-equal size: seen 2×1083, 3×1062, and 853/853/851 (in a different call whose path apparently had a lower limit). [O] The ~1200-byte threshold reproduces the observed splits; the exact rule and whether the relay accepts 1200-byte payloads from a client are `[I]`.
- **Receive:** dispatch by content — payload starting with an Annex-B start code = whole AU; first byte low bits 28 and second byte low bits 0 = Zalo fragment; otherwise fall back to RFC 6184 (single NAL / STAP-A / FU-A) for robustness. A lost first fragment drops the AU. [I] (RFC 6184 fallback was never needed live.)

```
packetise(au, max_payload = 1200):
    pt = 98 if au contains NAL type 5 else 97
    if len(au) <= max_payload: return [(pt, au, marker=1)]
    parts = ceil(len(au) / (max_payload - 2))
    chunk = ceil(len(au) / parts)
    for i, c in enumerate(split(au, chunk)):
        flags = (0x80 if i == 0 else 0) | (0x40 if last else 0)
        emit (pt, [0x1C, flags] + c, marker = last)
```

### 5.4 HEVC

`supportHevcDecode` non-zero in the remote `extendData` triggers `Peer::_changeVidCodec(5 /* H265 */)` in `libzrtc.so` when hardware HEVC encode is available. [I] A client without an HEVC decoder must advertise `supportHevcDecode: 0`. The official peer advertises `4`. [O]

## 6. RTCP

### 6.1 What the peer sends [O]

- Video: compound `SR (52 B: 28 + one 24-byte report block) + SDES (12 B, one chunk, no items) + PSFB/PLI`, from the shared SSRC, on the RTCP path with outer type `0x0F`.
- PLI about every 500 ms (487–520 ms) for the whole call when it cannot decode our video, and in one camera-off call starting ~27 s in, addressed to its own SSRC (unexplained). No FIR seen.
- Report blocks about our streams: the peer reports `fraction lost`, cumulative loss and highest seq for each of our SSRCs; `LSR = 0`, `DLSR = 0` throughout in calls where our SR was not delivered as RTCP.
- NACK (RTPFB) for our audio SSRC.

### 6.2 What a client should send [I]

- Every ~2 s: audio RR (`0x05`) from the audio SSRC with a report block about the peer (RFC 3550 A.1 loss stats, LSR/DLSR from the peer's last SR; jitter may be 0).
- When video is on: video `SR + empty SDES` (`0x0F`) from the video SSRC with a report block about the peer's video, optionally followed by REMB (`PT 206 fmt 15`, `"REMB"`, one SSRC; `RTCPSender::BuildREMB` in `libzrtc.so`).
- Because the relay may deliver to either socket, the previous client sent RTCP from the RTP socket to the RTCP address, and also to the RTP address when it differed. Whether that duplication is needed is `[U]`.

### 6.3 Keyframe requests

- **PLI works:** sending `RR (empty) + PSFB/PLI (fmt 1, media = peer SSRC)` with outer `0x0F` was followed at once by a PT 98 keyframe from the peer. [O]
- Send a PLI (at most every 500 ms) while no SPS/IDR has been decoded yet, or after a sequence gap in the peer's video. Without it the peer may send only P frames (seen: a whole call of P frames, 0 frames decodable). [O for the failure mode]
- On a received PLI or FIR, send SPS+PPS+IDR immediately on PT 98. [I] The peer keeps sending PLI at 2 Hz while it cannot decode what it gets. [O]

## 7. In-call negotiation packet [I]

`ZRTPPacket::initZRTPPacketIncallNegotiateInfo` (`libzrtc.so`), sent on the RTP socket: common header (`01 01`, `seq`, `subSeq 0`, u32 at offset 10, `token`, `cmd 0x0020`, `subCmd 0x14`) + `len u32 LE` + body:

```
seq      u32 BE
count    u8 = 2
H264DecCapacityInfo: 0B 02 D0 1E      (720 lines = 0x02D0, 30 fps = 0x1E)
NegoNetworkInfo:     29 01
```

`[U]` what the u32 at offset 10 must be (the previous client put the peer's 32-bit id there; the generic header calls it `fromId`).

## 8. Group calls (SFU) [I]

No live group call was recorded; everything here is from `ZaloCall.exe` disassembly.

- RTP from a group member carries extra one-byte header extensions (`RtpHeaderExtensionMap`):
  - audio: `BE DE 00 03` | ID 2 audio level (`20 xx`) | ID 13 Zalo NTP time (`D6` + 3 bytes NTP seconds (low 24 bits) + 4 bytes NTP fraction) | 2 pad;
  - video: `BE DE 00 05` | ID 3 abs-send-time (`32` + 3 bytes) | ID 5 transport seq (`51` + 2 bytes) | ID 12 simulcast layer info (`C2` + layer byte + 2-byte picture id) | ID 13 NTP (`D6` + 7 bytes) | 1 pad.
- Control packets (common header type `0x01`, `cmd 0x0020`, body after `u16 LE length`):
  - `subCmd 0x0C` layers announcement: `count u8` + `count × (qualityId u8, bitrate u32 LE kbps, width u16 LE, height u16 LE)`; the previous client announced `(0,120,160,120) (3,300,320,240) (6,600,640,480)` (`qualityId = 3·s`).
  - `subCmd 0x0D` subscribe video: `count u8` + `count × (uid32 u32 LE, qualityId u8)`. Request `qualityId 0` (base layer); higher layers are dropped when the sender only sends `S=0`. A received 0x0D naming our uid tells us the layer to send and should trigger a keyframe.
- `cmd 0x0032` room updated (`02 7E …`): `res i32` at offset 21, then a list of member 32-bit ids (u32 LE) from offset 25; subscribe to new members and send a keyframe.
- Received `cmd 0x0020` packets also reveal members by their `fromId`.

## 9. Audio → video upgrade

Same call, no new call id: the engine's `switchToVideoCall` (`peercallzrtpimpl.cpp`) checks in-call state, rejects if already video, requires a valid zrtc config, re-inits audio, then starts video coding/capture; the other side runs `switchToVideoCallAnswer` / `acceptStartCamera`. Related strings: `SwitchToVideo`, `updateLocalCameraState`, `updatePartnerCameraState`, `onCallVideoState`, JSON `video.cameraState`. [O] (strings) The signaling carrier is probably `conf` (12006) with `isOnCam=1`, `[U]`. Media-wise it means starting video RTP on PT 97/98 with the same relay token. [I]

## 10. Engine capability strings [O]

`zrtc_core` is a WebRTC fork. Codecs: H.264 (OpenH264, x264, OpenH264SVC), H.265 (FFmpeg, x265, `hevc_mediacodec`), VP8. Present: `RtpPacketizerH264` with FU-A and STAP-A, NACK, PLI (`Incoming PLI from SSRC`), FIR, REMB, transport feedback, RTX, ULPFEC/RED, `abs-send-time`, `keyFrameIntervalMs`, `requestKeyFrameRateMs`, `Use ZRTP packetization mode`. The RFC 6184 packetizer exists in the binary, but the relay path observed live uses the Annex-B AU format of §5.3.

## Open questions

- [U] Whether TWCC (extension ID 5 + RTCP transport feedback `PT 205 fmt 15`) is required for the peer to judge the link healthy.
- [U] Whether audio and video must share one SSRC.
- [U] Maximum payload the relay accepts from a client (1200 assumed; peer used up to 1212, and 853 in one call).
- [U] Exact fragment-count rule (`ceil(len/1200)` reproduces observations).
- [U] Payload types for VP8 (and confirmation of 121/122/125/126) on the wire.
- [U] Meaning of the 2-byte value pattern in the video stream's extension ID 5 (non-contiguous values).
- [U] Whether a client must answer NACKs; contents of the `01 01` / `02 01` relay control datagrams beyond the `libzrtc.so` interpretation.
- [U] Why the peer sent PLI addressed to its own SSRC in a camera-off call.
- [U] Contradiction: a PLI sent with outer `0x0F` (fourth capture) got an immediate keyframe, while the fifth-capture analysis concludes earlier RTCP was sent with `0x0D` and never reached the peer as RTCP; which outer byte that successful PLI used is not recorded unambiguously.
