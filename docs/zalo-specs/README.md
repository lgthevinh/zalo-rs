# Zalo specs

Implementation-agnostic notes on how Zalo's servers and official PC/Web client behave, learned by reverse engineering. They are written so a client can be built from scratch in any language, and they are the evidence behind every protocol change (see `AGENTS.md` → *Do not silently change protocol behaviour*).

## Evidence tags

| Tag | Meaning |
|---|---|
| `[O]` | Observed: live-verified, or read directly from the official Zalo bundle/binary. |
| `[I]` | Implemented in the previous client, but no recorded live evidence. Treat as a strong hint, not a fact. |
| `[U]` | Unknown or contradictory. Every doc ends with an *Open questions* list. |

## Where to start

1. [auth/service-discovery.md](auth/service-discovery.md): which host serves what.
2. [auth/session.md](auth/session.md): what every request carries.
3. Encryption, by request kind:
   - Login-group calls: [security/param-cipher.md](security/param-cipher.md) + [security/signkey.md](security/signkey.md).
   - Post-login data calls: [security/payload-encryption.md](security/payload-encryption.md) (session key `zpw_enk`, no signkey).
4. [auth/qr-login.md](auth/qr-login.md), then [api/conversations.md](api/conversations.md) and [realtime/websocket.md](realtime/websocket.md).

## Index

| Category | Doc | Contents |
|---|---|---|
| **auth**: login & session | [service-discovery.md](auth/service-discovery.md) | `getServerInfo`, service-map keys, default vs live hosts, host failover, WebSocket hosts. |
| | [session.md](auth/session.md) | Common params, headers, cookies, `getLoginInfo`, what to persist, device id, expiry. |
| | [qr-login.md](auth/qr-login.md) | QR token → wait for scan → confirm → session. |
| | [password-login.md](auth/password-login.md) | `loginByPasswv2`, PasswordCipher, captcha, other login endpoints. |
| **security**: encryption & signing | [param-cipher.md](security/param-cipher.md) | ParamCipher v2 algorithm, static bundle constants. |
| | [signkey.md](security/signkey.md) | MD5 signature, tag naming, signed field sets, test vectors. |
| | [payload-encryption.md](security/payload-encryption.md) | Session AES with `zpw_enk` for requests and responses. |
| **api**: REST endpoints | [conversations.md](api/conversations.md) | Conversation list, last messages, long-poll, pin, unread mark, mute, hide, delete. |
| | [contacts.md](api/contacts.md) | Friends, profiles, phone lookup, friend requests, online status. |
| | [groups.md](api/groups.md) | Group list, info, members, avatars. |
| | [messages-send.md](api/messages-send.md) | Text, rich text, links, quotes, forwards (1-1 and group). |
| | [message-actions.md](api/message-actions.md) | Reactions, delete-for-me, recall. |
| | [history-sync.md](api/history-sync.md) | Group and My Cloud history paging, sync from phone. |
| | [feed.md](api/feed.md) | Social profile and timeline album; what is unknown about the feed. |
| **realtime** | [websocket.md](realtime/websocket.md) | `zpw_ws` connection, frame codec, decryption, keepalive, reconnect, HTTP long-pollers. |
| | [events.md](realtime/events.md) | Command ID catalogue and event payloads. |
| **media** | [uploads.md](media/uploads.md) | Chunked photo, video and file upload, then send. |
| | [downloads.md](media/downloads.md) | CDN URL shapes, avatars, download naming, safety bounds. |
| | [stickers.md](media/stickers.md) | Sticker IDs, store, search (Vietnamese keyword normalisation), sending, image URLs. |
| **call**: voice & video | [signaling.md](call/signaling.md) | `/api/voicecall/*`, `extendData`, incoming calls, UDP relay protocol, state machines. |
| | [media-transport.md](call/media-transport.md) | Relay framing, RTP/RTCP, Opus, Zalo's H.264 packetisation. |
| | [video-call.md](call/video-call.md) | Video-specific engine facts and dated live-capture findings. |
| | [voip-log.md](call/voip-log.md) | Decoding and reading ZaloCall.exe's `voip.log`. |
| **client-rules**: behaviour learned from Zalo | [message-model.md](client-rules/message-model.md) | Wire message fields, `msgType` vocabulary, content shapes. |
| | [message-identity.md](client-rules/message-identity.md) | Matching and merging copies of the same message. |
| | [threads.md](client-rules/threads.md) | One entry per conversation, names, avatars, previews, startup sequence. |
| | [unread.md](client-rules/unread.md) | Unread counts from server read cursors, mentions, badges. |
| | [reactions.md](client-rules/reactions.md) | Reaction codes and loss-free aggregation. |
| | [errors.md](client-rules/errors.md) | Every known `error_code`, HTTP failures, retry and backoff. |
| **tooling** | [capture.md](tooling/capture.md) | Capture official traffic and verify it against these specs. |

## Adding a doc

- Put it in the matching category, or create a new folder when its first doc arrives.
- Start with `# Title` and a one-paragraph purpose; end with `## Open questions`.
- Describe endpoints with the template used in `api/` (request, encoding, params, response, errors, notes).
- Keep docs implementation-agnostic: no references to any client's source code.
- Tag claims `[O]`/`[I]`/`[U]`, and record the date and Zalo version for new observations.
- Sanitise captures before committing: no cookies, `zpw_sek`, `zpw_enk`, IMEI, phone numbers or user/group IDs.
