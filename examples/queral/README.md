# QUERAL

QUERAL is a realtime communications workspace for the AGILANG native framework.

It exercises:

- WebSocket upgrade and echo on `/ws`
- Authenticated WebRTC signaling on `/api/webrtc/register`, `/api/webrtc/signal`, and `/api/webrtc/poll`
- Browser `RTCPeerConnection` offer, answer, and ICE exchange between two tabs
- Meeting-style invite links backed by deterministic peer identity generation

Run it from this directory:

```powershell
agilang serve
```

Then open:

```text
http://127.0.0.1:8080/
```

Basic test flow:

1. Open QUERAL in two browser tabs.
2. Register or log in through the built-in auth routes.
3. Create a meeting in the first tab and copy the invite link.
4. Open the invite link in the second tab.
5. Enter the waiting room on both sides and start listening.
6. Start the call from one tab and let the other auto-answer.
