# ZaloCall.exe `voip.log`

How to decode and read the log written by Zalo PC's native call engine (`plugins\capture\ZaloCall.exe`). It is the best window into the official engine's state machine and the main tool for comparing a re-implementation against the official client.

Evidence tags: `[O]` observed in decoded logs or read from the binary; `[I]` inferred; `[U]` unknown. Reference: `ZaloCall.exe` 26.10.x (x86, unpacked); log spanning 2025-11-15 → 2026-10-27, ~1900 lines.

## 1. Location and rotation [O]

- Path: `%APPDATA%\ZaloData\cal\voip.log`.
- Rotates at ~2 MB by renaming with an `.old.` infix.
- Contains **state events only**, no HTTP request bodies, no packet payloads.

## 2. Encoding [O]

Not encryption: a fixed 128-entry substitution table baked into the binary.

- Line format: `<timestamp>\t<hex>`; timestamp format `%Y-%m-%d %X` (local time). Multi-line events continue on lines that are pure hex **without** a timestamp.
- Each plaintext byte `p` is written as `T[p & 0x7F]`, hex-printed.
- `T` is a bijection on 0..127. Its 128 bytes are these 32 little-endian dwords, in order (byte 0 of the first dword = `T[0x00]`):

```
0x29354f63 0x162f1d42 0x101c0f65 0x55121750 0x2d222543 0x381b6d13 0x7e0b6e15 0x7b66184e
0x5d522608 0x73370964 0x51726b14 0x190a2153 0x6c0e1a0d 0x5e3b0778 0x2028006f 0x4d7d1130
0x5b543e31 0x48751e04 0x057a3a27 0x79583d23 0x68572c06 0x560c6003 0x2b770161 0x49703c34
0x4167334c 0x765c7c1f 0x5a474639 0x407f0274 0x5f592a24 0x4b2e3f6a 0x36713269 0x4a454462
```

- Where it lives in the binary: log-init function at VA `0x7f8380` (file offset `0x3f7780`), the only xref of the `\voip.log` string (VA `0x1235260`) is at VA `0x7f83b0`; the table is built on the stack from VA `0x7f86d1` to `0x7f87c4` and copied to global `0x15e55e0`; the directory argument is cached in global `0x15b9024`. Re-derive the table from there if a new build changes it (check it is still a bijection).

### Decoder

```
T = bytes of the 32 dwords above, little-endian, concatenated   # len 128
INV = identity[0..256); for p in 0..128: INV[T[p]] = p

decode_hex(h):
    out = ""
    for b in bytes_from_hex(h):
        if b < 128 or INV[b] != b: out += chr(INV[b])
        else: out += "<%02X>" % b            # byte outside the table: keep visible
    return out

decode_line(line):
    if "\t" in line:
        ts, hex = split_once(line, "\t")
        return (ts, decode_hex(hex)) if is_even_length_hex(hex) else (ts, None)
    if is_even_length_hex(line.strip()):
        return (None, decode_hex(line.strip()))   # continuation of previous event
    return (None, None)                           # not a log line
```

Print `ts |plain|` for timestamped lines and `    |plain|` for continuations; tail mode = re-read from the last offset, restart at 0 if the file shrank (rotation).

## 3. Official caller sequence [O]

Seen three times, 1-1 audio:

```
cmd makecall → request make call 1-1 → [SIG]requestCall (plain/ui/devc/state/ui show)
→ cmd makeCall done → [SIG]receive 401 resp
→ makeCall isVideo = 0 → _initZrtcConfig 1..4 (+ congestionThread Start)
→ _initAudioDevice begin/end → startNoAudio → _handleStartAudio startAudio ret = 0
→ setCallState state = 1, curState = 6 → onMakeCall → _scheduleHandler success
→ _doInitZRTP size = 0 → _sendEchoRequestToServers startSeq = 0, endSeq = 120..160, timeout = 2000
→ _handleSelectServersTimeout size = 0 → _doInitZRTP size = 4
→ _handleInitZRTPSuccessful index = 0, size = 4 → onInitZrtpWithServer
→ [SIG]receive 416 resp (ui:stateChange 7) → [SIG]receive ring (ui:stateChange 8)
→ setCallState state = 3, curState = 1 → onCallState: 3
→ [SIG]receive answer → updateCallerInfo → state = 4, curState = 3 → onCallState: 4 (ui:stateChange 9)
→ state = 4, curState = 4 → state = 5, curState = 4 → "Call to confirmed. _handleStartCall"
→ onCallUpdateP2PStatus → onCallState: 5 → ui:stateChange 1 → onCallAudioState, 10   (media up)
```

`endSeq` of the echo probe was 120, 140 or 160 across runs: a server-count heuristic, not a constant. [I] What each step means on the wire: [signaling.md](signaling.md) §8–9.

## 4. Official callee sequences [O]

- **Ringing:** `recv incoming call req 1-1` → `[SIG]recvIncomCall` → device list → `ui:stateChange, 10` → `ZCallMainWindowVideo_v2::showCall` → `recvAliasName` → `backgroundSize 452-695` → `ZCalleeWindow::showCall`.
- **Accept:** `btnAcceptClick` → `sendActionLog: 1150311` → `sendAction: 5` → `UiAnswerCallEvent, 1` → `ui:stateChange, 13` → `incomingCall isVideo = 0` → `_initZrtcConfig 1..4` → `setCallState state = 2, curState = 6` → `_scheduleHandler success` → `_doInitZRTP size = N` → …
- **Reject:** `btnRejectClick` → `sendActionLog: 1150303` → `sendAction: 8` → `UiAnswerCallEvent, 0`.
- **Caller cancels while dialing:** `vidBtnEndcallClick` → `sendActionLog: 1150201` → `UiEndCallEvent, 6`; ending while ringing → `UiEndCallEvent, 8`.
- **Callee with a thin invite:** when the invite's `extendData` offered a single server/p2p entry, the official callee logged `_doInitZRTP size = 1`, then `endCall stopDevices = 0` → `disconnect 3, 4, 6, 7` → `onCallState: 6` within ~1 s, with no echo probe and no `InitZRTPSuccessful`. A healthy setup reaches `size = 4`. [O]

## 5. Reading guide: lines that look alarming but are not [O]

| Line | Meaning |
|---|---|
| `setCallState end` | Exit log of the state setter; follows **every** `setCallState state = N`. Not an end-of-call marker. |
| `ZErr:<pid> calle recv answer` | Fires on the callee ~0–1 s into every incoming call, answered or not. Benign. |
| `ZErr:<pid> Undefined recv cmd, 408` | The caller engine has no handler for `answerack`; logged right after `Call to confirmed`. Call proceeds. |
| `ZErr:<pid> not dialing` | A 416/request arrived after the call record died. Also proves 416 reaches the callee engine. |
| `ZErr:<pid> invalid state` | `recvAnsAck` while still ringing (stray ack). |
| `ZErr:<pid> no any call` | endcall / recvEndCall with no live record. |
| `no default vid inf`, `No default camera` | Device enumeration noise. |
| `showNoti <uid>` | Missed-call notification for that user. |

The real teardown marker is the run `disconnect 3 / 4 / 6 / 7` followed by `onCallState: 6`.

## 6. State codes [O]

| Family | Codes |
|---|---|
| `ui:stateChange` | 1 in-call, 2 hidden/reset, 6 outgoing, 7 dial upgraded (after 416), 8 ringing, 9 answer received, 10 incoming, 13 accepting |
| `onCallState` | 3 ringing, 4 answer, 5 confirmed, 6 torn down |
| `setCallState state` | 1 dialing, 2 callee accepting, 3 ringing, 4 answer, 5 confirmed |
| `disconnect` | 3, 4, 6, 7 = teardown sub-steps |

## 7. Other useful log strings [O]

`_parseExtendData srtpMode = %d`, `[codecName = %s, payload = %d, frameTime = %d, dynamicFPT = %d]`, `partner using old version, not support hevc decode`, `partner support hevc decode = %d`, `Use ZRTP packetization mode`, `Create VideoRtpRtcp: _callType=`, `Begin switchToVideoCall(): startCamera=`. See [video-call.md](video-call.md).

## Open questions

- [U] A healthy official callee's `_doInitZRTP size` (no successful callee session in the decoded log).
- [U] Whether `size` counts relay servers or p2p entries from the invite's `extendData`.
- [U] The official 402 body (the log never carries bodies).
