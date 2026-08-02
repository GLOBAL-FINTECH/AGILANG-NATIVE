document.documentElement.dataset.agilang = "ready";
console.info("AGILANG application assets loaded");

// 1. WebSocket integration
const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
const socket = new WebSocket(`${wsProtocol}//${window.location.host}`);

socket.onopen = () => {
    console.info("WebSocket connected successfully to AGILANG Native Server!");
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Connected";
        wsBadge.className = "badge success";
    }
};

socket.onmessage = (event) => {
    console.log("WebSocket message received:", event.data);
};

socket.onerror = (error) => {
    console.error("WebSocket error:", error);
    const wsBadge = document.getElementById("ws-status");
    if (wsBadge) {
        wsBadge.textContent = "Error";
        wsBadge.className = "badge error";
    }
};

// 2. WebRTC integration with built-in STUN
const configuration = {
    iceServers: [
        { urls: `stun:${window.location.hostname}:3478` }
    ]
};

const peerConnection = new RTCPeerConnection(configuration);

peerConnection.onicecandidate = (event) => {
    if (event.candidate) {
        console.info("Local STUN server resolved ICE candidate:", event.candidate.candidate);
        const rtcBadge = document.getElementById("rtc-status");
        if (rtcBadge) {
            rtcBadge.textContent = "ICE Resolved";
            rtcBadge.className = "badge success";
        }
    }
};

peerConnection.onconnectionstatechange = () => {
    console.info("WebRTC Connection State changed to:", peerConnection.connectionState);
};

peerConnection.createDataChannel("agi-sync");
peerConnection.createOffer()
    .then(offer => peerConnection.setLocalDescription(offer))
    .catch(err => console.error("WebRTC offer error:", err));
