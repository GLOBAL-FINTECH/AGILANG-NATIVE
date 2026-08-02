const state = {
  auth: null,
  csrf: '',
  ws: null,
  pollTimer: null,
  peerConnection: null,
  dataChannel: null,
  localStream: null,
  remoteStream: null,
  pendingCandidates: [],
  audioEnabled: true,
  videoEnabled: true,
  inviteUrl: '',
};

const ids = [
  'auth-state', 'auth-user', 'auth-role', 'webrtc-mode', 'active-peers', 'ws-state',
  'poll-state', 'media-state', 'runtime-json', 'ws-log', 'signal-log', 'pc-log',
  'pc-signaling', 'pc-connection', 'pc-ice', 'pc-channel', 'peer-id', 'target-peer-id',
  'ws-message', 'dc-message', 'local-video', 'remote-video', 'notifications',
  'chat-messages', 'local-label', 'remote-label', 'display-name', 'meeting-id',
  'invite-link', 'peer-summary', 'target-summary'
];
const el = Object.fromEntries(ids.map(id => [id, document.getElementById(id)]));
const realtimePage = Boolean(el['meeting-id'] && el['local-video'] && el['notifications']);

function setText(node, value) {
  if (node) node.textContent = value;
}

function appendLog(node, message) {
  if (!node) return;
  const timestamp = new Date().toLocaleTimeString();
  node.textContent = `[${timestamp}] ${message}\n${node.textContent}`.trim();
}

function notify(level, message) {
  const container = el.notifications;
  if (!container) return;
  if (container.firstElementChild?.classList.contains('muted')) {
    container.innerHTML = '';
  }
  const item = document.createElement('div');
  item.className = `notification ${level || 'info'}`;
  item.textContent = `${new Date().toLocaleTimeString()}  ${message}`;
  container.prepend(item);
}

function appendChat(kind, message) {
  const box = el['chat-messages'];
  if (!box) return;
  if (box.firstElementChild?.classList.contains('system') && box.children.length === 1) {
    box.innerHTML = '';
  }
  const entry = document.createElement('div');
  entry.className = `chat-entry ${kind}`;
  entry.textContent = message;
  box.appendChild(entry);
  box.scrollTop = box.scrollHeight;
}

function sanitizeSegment(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
}

function meetingId() {
  return el['meeting-id'].value.trim();
}

function localPeerId() {
  return el['peer-id'].value.trim();
}

function targetPeerId() {
  return el['target-peer-id'].value.trim();
}

function currentDisplayName() {
  return el['display-name'].value.trim();
}

function buildLocalPeerId() {
  const meeting = sanitizeSegment(meetingId());
  const identity = sanitizeSegment(state.auth?.user?.email || currentDisplayName() || state.auth?.user?.name || 'guest');
  return meeting && identity ? `meeting-${meeting}-${identity}` : '';
}

function refreshPeerSummary() {
  setText(el['peer-summary'], localPeerId() || 'not-ready');
  setText(el['target-summary'], targetPeerId() || 'waiting');
}

function updateInviteLink() {
  const meeting = meetingId();
  const localPeer = localPeerId();
  const url = new URL(window.location.href);
  url.searchParams.set('meeting', meeting);
  if (localPeer) {
    url.searchParams.set('host', localPeer);
  } else {
    url.searchParams.delete('host');
  }
  state.inviteUrl = meeting ? url.toString() : '';
  el['invite-link'].value = state.inviteUrl;
}

function syncMeetingState() {
  const computedPeer = buildLocalPeerId();
  if (computedPeer) {
    el['peer-id'].value = computedPeer;
  }
  refreshPeerSummary();
  updateInviteLink();
}

function consumeUrlState() {
  const url = new URL(window.location.href);
  const meeting = url.searchParams.get('meeting');
  const host = url.searchParams.get('host');
  const display = url.searchParams.get('name');
  if (meeting && !meetingId()) {
    el['meeting-id'].value = meeting;
  }
  if (display && !currentDisplayName()) {
    el['display-name'].value = display;
  }
  if (host && !targetPeerId()) {
    el['target-peer-id'].value = host;
  }
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, {
    credentials: 'same-origin',
    headers: {
      Accept: 'application/json',
      ...(options.headers || {}),
    },
    ...options,
  });
  const text = await response.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch (_) {
    json = { raw: text };
  }
  return { response, json, text };
}

async function refreshRuntime() {
  const [authResult, webrtcResult] = await Promise.all([
    fetchJson('/api/auth/me'),
    fetchJson('/api/webrtc/status'),
  ]);

  state.auth = authResult.json;
  const authenticated = authResult.response.ok && authResult.json?.authenticated;
  if (authResult.response.status === 401 && realtimePage && meetingId()) {
    const returnTo = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    window.location.replace(`/login?return_to=${encodeURIComponent(returnTo)}`);
    return;
  }
  if (authenticated && !currentDisplayName()) {
    el['display-name'].value = authResult.json.user.name || authResult.json.user.email;
  }

  syncMeetingState();

  setText(el['auth-state'], authenticated ? 'Authenticated' : 'Guest');
  setText(el['auth-user'], authenticated ? authResult.json.user.email : 'Guest');
  setText(el['auth-role'], authenticated ? authResult.json.user.role : 'guest');

  setText(
    el['webrtc-mode'],
    webrtcResult.json?.peer_connection ? 'WebRTC native' : (webrtcResult.json?.signaling ? 'WebRTC signaling-only' : 'WebRTC disabled')
  );
  setText(el['active-peers'], String(webrtcResult.json?.active_peers ?? 0));
  setText(el['poll-state'], state.pollTimer ? 'Running' : 'Stopped');
  setText(el['media-state'], state.localStream ? 'Media live' : 'Media off');
  refreshPeerSummary();

  el['runtime-json'].textContent = JSON.stringify({
    auth: authResult.json,
    webrtc: webrtcResult.json,
    meeting: {
      id: meetingId(),
      local_peer: localPeerId(),
      target_peer: targetPeerId(),
      invite_url: state.inviteUrl,
    }
  }, null, 2);
}

async function refreshCsrf() {
  const path = state.auth?.authenticated ? '/dashboard' : '/login';
  const response = await fetch(path, { credentials: 'same-origin' });
  const html = await response.text();
  const match = html.match(/name="_csrf"\s+value="([^"]+)"/);
  state.csrf = match ? match[1] : '';
  appendLog(el['signal-log'], state.csrf ? `CSRF token loaded from ${path}` : `CSRF token not found in ${path}`);
  notify(state.csrf ? 'success' : 'warn', state.csrf ? `CSRF token loaded from ${path}` : 'CSRF token not found');
}

function ensureAuthenticated() {
  if (state.auth?.authenticated) return true;
  appendLog(el['signal-log'], 'Login is required for WebRTC signaling.');
  notify('warn', 'Login is required for signaling.');
  return false;
}

function ensureCsrf() {
  if (state.csrf) return true;
  appendLog(el['signal-log'], 'Fetch CSRF first.');
  notify('warn', 'Fetch CSRF before signaling.');
  return false;
}

function ensureMeeting() {
  if (meetingId()) return true;
  notify('warn', 'Create or join a meeting first.');
  return false;
}

async function ensureLocalMedia() {
  if (state.localStream) return state.localStream;
  const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
  state.localStream = stream;
  el['local-video'].srcObject = stream;
  setText(el['local-label'], `${stream.getVideoTracks().length ? 'camera' : 'video-off'} / ${stream.getAudioTracks().length ? 'mic' : 'audio-off'}`);
  setText(el['media-state'], 'Media live');
  notify('success', 'Local camera and microphone started.');
  attachLocalTracks();
  return stream;
}

function attachLocalTracks() {
  if (!state.peerConnection || !state.localStream) return;
  const existing = state.peerConnection.getSenders().map(sender => sender.track && sender.track.id).filter(Boolean);
  for (const track of state.localStream.getTracks()) {
    if (!existing.includes(track.id)) {
      state.peerConnection.addTrack(track, state.localStream);
    }
  }
}

function createMeeting() {
  const generated = String(Math.floor(100000000 + Math.random() * 900000000));
  el['meeting-id'].value = generated;
  syncMeetingState();
  notify('success', `Meeting ${generated} created.`);
}

function joinMeeting() {
  if (!ensureMeeting()) return;
  syncMeetingState();
  const url = new URL(window.location.href);
  url.searchParams.set('meeting', meetingId());
  if (localPeerId()) {
    url.searchParams.set('host', localPeerId());
  }
  window.history.replaceState({}, '', url);
  notify('success', `Joined meeting ${meetingId()}.`);
}

async function copyInvite() {
  if (!state.inviteUrl) {
    notify('warn', 'Create a meeting first.');
    return;
  }
  try {
    await navigator.clipboard.writeText(state.inviteUrl);
    notify('success', 'Invite link copied.');
  } catch (_) {
    el['invite-link'].select();
    notify('warn', 'Clipboard access failed. Copy the invite link manually.');
  }
}

async function registerPeer() {
  if (!ensureAuthenticated() || !ensureCsrf() || !ensureMeeting()) return;
  syncMeetingState();
  const peerId = localPeerId();
  if (!peerId) {
    appendLog(el['signal-log'], 'Local peer id is required.');
    notify('warn', 'Local peer id is required.');
    return;
  }

  const result = await fetchJson('/api/webrtc/register', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': state.csrf,
    },
    body: JSON.stringify({ peer_id: peerId }),
  });

  appendLog(el['signal-log'], `${result.response.status} register -> ${JSON.stringify(result.json)}`);
  notify(result.response.ok ? 'success' : 'error', result.response.ok ? 'You entered the waiting room.' : `Peer register ${result.response.status}`);
  await refreshRuntime();
}

async function sendSignal(kind, payload) {
  if (!ensureAuthenticated() || !ensureCsrf()) return false;
  const from = localPeerId();
  const to = targetPeerId();
  if (!from || !to || !payload) {
    appendLog(el['signal-log'], 'Meeting peers are not ready.');
    notify('warn', 'Meeting peers are not ready.');
    return false;
  }

  const result = await fetchJson('/api/webrtc/signal', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': state.csrf,
    },
    body: JSON.stringify({ from, to, kind, payload }),
  });

  appendLog(el['signal-log'], `${result.response.status} ${kind} -> ${JSON.stringify(result.json)}`);
  notify(result.response.ok ? 'success' : 'error', `${kind} signal ${result.response.status}`);
  return result.response.ok;
}

async function pollSignals() {
  if (!ensureAuthenticated()) return;
  const peerId = localPeerId();
  if (!peerId) {
    appendLog(el['signal-log'], 'Join the meeting before polling.');
    return;
  }

  const result = await fetchJson(`/api/webrtc/poll?peer_id=${encodeURIComponent(peerId)}`);
  if (!result.response.ok) {
    appendLog(el['signal-log'], `${result.response.status} poll -> ${JSON.stringify(result.json)}`);
    notify('error', `Poll failed with ${result.response.status}`);
    return;
  }

  const message = result.json?.message;
  if (!message) return;
  appendLog(el['signal-log'], `received ${message.kind} from ${message.from}`);
  notify('info', `Received ${message.kind} from ${message.from}`);
  await handleSignalEnvelope(message);
}

function togglePolling() {
  if (state.pollTimer) {
    clearInterval(state.pollTimer);
    state.pollTimer = null;
    setText(el['poll-state'], 'Stopped');
    appendLog(el['signal-log'], 'Polling stopped.');
    notify('warn', 'Signaling polling stopped.');
    document.getElementById('peer-autopoll').textContent = 'Start listening';
    return;
  }
  state.pollTimer = setInterval(() => {
    pollSignals().catch(error => appendLog(el['signal-log'], `poll error: ${error.message}`));
  }, 1200);
  setText(el['poll-state'], 'Running');
  appendLog(el['signal-log'], 'Polling started.');
  notify('success', 'Signaling polling started.');
  document.getElementById('peer-autopoll').textContent = 'Stop listening';
}

function updatePeerConnectionState() {
  const pc = state.peerConnection;
  setText(el['pc-signaling'], pc?.signalingState || 'idle');
  setText(el['pc-connection'], pc?.connectionState || 'idle');
  setText(el['pc-ice'], pc?.iceConnectionState || 'idle');
  setText(el['pc-channel'], state.dataChannel?.readyState || 'not-open');
}

function createPeerConnection() {
  const pc = new RTCPeerConnection();
  state.remoteStream = new MediaStream();
  el['remote-video'].srcObject = state.remoteStream;

  pc.onconnectionstatechange = () => {
    updatePeerConnectionState();
    notify('info', `Connection state: ${pc.connectionState}`);
  };
  pc.onsignalingstatechange = () => updatePeerConnectionState();
  pc.oniceconnectionstatechange = () => updatePeerConnectionState();
  pc.onicecandidate = async event => {
    updatePeerConnectionState();
    if (!event.candidate) return;
    await sendSignal('ice-candidate', JSON.stringify(event.candidate));
    appendLog(el['pc-log'], `local ICE -> ${event.candidate.candidate}`);
  };
  pc.ontrack = event => {
    for (const track of event.streams[0].getTracks()) {
      state.remoteStream.addTrack(track);
    }
    setText(el['remote-label'], 'live');
    notify('success', 'Remote media track received.');
  };
  pc.ondatachannel = event => {
    state.dataChannel = event.channel;
    wireDataChannel(event.channel);
    appendLog(el['pc-log'], `remote data channel received: ${event.channel.label}`);
    notify('success', `Remote data channel ${event.channel.label} received.`);
    updatePeerConnectionState();
  };

  state.peerConnection = pc;
  attachLocalTracks();
  return pc;
}

function resetPeerConnection() {
  if (state.dataChannel) {
    try { state.dataChannel.close(); } catch (_) {}
  }
  if (state.peerConnection) {
    try { state.peerConnection.close(); } catch (_) {}
  }
  state.pendingCandidates = [];
  state.dataChannel = null;
  createPeerConnection();
  const channel = state.peerConnection.createDataChannel('queral-chat');
  state.dataChannel = channel;
  wireDataChannel(channel);
  setText(el['remote-label'], 'waiting');
  updatePeerConnectionState();
  appendLog(el['pc-log'], 'Peer connection reset.');
}

function wireDataChannel(channel) {
  channel.onopen = () => {
    appendLog(el['pc-log'], `data channel open: ${channel.label}`);
    notify('success', `Data channel open: ${channel.label}`);
    updatePeerConnectionState();
  };
  channel.onclose = () => {
    appendLog(el['pc-log'], `data channel closed: ${channel.label}`);
    notify('warn', `Data channel closed: ${channel.label}`);
    updatePeerConnectionState();
  };
  channel.onmessage = event => {
    appendLog(el['pc-log'], `data channel <- ${event.data}`);
    appendChat('remote', event.data);
    notify('info', 'New live message received.');
  };
}

async function flushPendingCandidates() {
  if (!state.peerConnection?.remoteDescription) return;
  while (state.pendingCandidates.length) {
    const candidate = state.pendingCandidates.shift();
    await state.peerConnection.addIceCandidate(candidate);
    appendLog(el['pc-log'], 'applied queued ICE candidate');
  }
}

async function createOffer() {
  if (!ensureAuthenticated() || !ensureCsrf() || !ensureMeeting()) return;
  if (!localPeerId() || !targetPeerId()) {
    notify('warn', 'Open the invite in the second browser so a target peer is known.');
    return;
  }
  await ensureLocalMedia();
  if (!state.peerConnection) {
    createPeerConnection();
  }
  attachLocalTracks();
  const offer = await state.peerConnection.createOffer();
  await state.peerConnection.setLocalDescription(offer);
  appendLog(el['pc-log'], 'local offer created');
  notify('success', 'Local offer created.');
  await sendSignal('offer', offer.sdp || '');
  updatePeerConnectionState();
}

async function sendAnswer() {
  const answer = await state.peerConnection.createAnswer();
  await state.peerConnection.setLocalDescription(answer);
  appendLog(el['pc-log'], 'local answer created');
  notify('success', 'Local answer created.');
  await sendSignal('answer', answer.sdp || '');
}

async function handleSignalEnvelope(message) {
  if (!state.peerConnection) {
    createPeerConnection();
  }

  if (message.from && message.from !== targetPeerId()) {
    el['target-peer-id'].value = message.from;
    refreshPeerSummary();
    updateInviteLink();
  }

  if (message.kind === 'offer') {
    await ensureLocalMedia();
    appendLog(el['pc-log'], 'remote offer received');
    await state.peerConnection.setRemoteDescription({ type: 'offer', sdp: message.payload });
    await flushPendingCandidates();
    if (document.getElementById('auto-answer').checked) {
      await sendAnswer();
    }
  } else if (message.kind === 'answer') {
    appendLog(el['pc-log'], 'remote answer received');
    await state.peerConnection.setRemoteDescription({ type: 'answer', sdp: message.payload });
    await flushPendingCandidates();
  } else if (message.kind === 'ice-candidate') {
    const candidateInit = JSON.parse(message.payload);
    if (state.peerConnection.remoteDescription) {
      await state.peerConnection.addIceCandidate(candidateInit);
      appendLog(el['pc-log'], 'remote ICE candidate applied');
    } else {
      state.pendingCandidates.push(candidateInit);
      appendLog(el['pc-log'], 'remote ICE candidate queued');
    }
  } else if (message.kind === 'hangup') {
    appendLog(el['pc-log'], 'remote hangup received');
    notify('warn', 'Remote peer ended the call.');
    resetPeerConnection();
  }
  updatePeerConnectionState();
}

async function hangupCall() {
  await sendSignal('hangup', 'hangup');
  resetPeerConnection();
  notify('warn', 'Call ended locally.');
}

function connectWebSocket() {
  if (state.ws && state.ws.readyState === WebSocket.OPEN) {
    state.ws.close();
    return;
  }

  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${protocol}//${location.host}/ws`);
  state.ws = ws;

  ws.addEventListener('open', () => {
    setText(el['ws-state'], 'WebSocket live');
    appendLog(el['ws-log'], 'connected to /ws');
    notify('success', 'WebSocket connected.');
    document.getElementById('ws-connect').textContent = 'Disconnect socket';
  });
  ws.addEventListener('message', event => {
    appendLog(el['ws-log'], `echo <- ${event.data}`);
    notify('info', `WebSocket message: ${event.data}`);
  });
  ws.addEventListener('close', () => {
    setText(el['ws-state'], 'WebSocket closed');
    appendLog(el['ws-log'], 'socket closed');
    notify('warn', 'WebSocket closed.');
    document.getElementById('ws-connect').textContent = 'Connect socket';
  });
  ws.addEventListener('error', () => {
    setText(el['ws-state'], 'WebSocket error');
    appendLog(el['ws-log'], 'socket error');
    notify('error', 'WebSocket error.');
  });
}

function sendWebSocketMessage() {
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) {
    appendLog(el['ws-log'], 'connect the socket first');
    notify('warn', 'Connect the socket first.');
    return;
  }
  const message = el['ws-message'].value;
  state.ws.send(message);
  appendLog(el['ws-log'], `echo -> ${message}`);
}

function sendDataChannelMessage() {
  if (!state.dataChannel || state.dataChannel.readyState !== 'open') {
    appendLog(el['pc-log'], 'data channel is not open');
    notify('warn', 'Data channel is not open.');
    return;
  }
  const message = el['dc-message'].value;
  state.dataChannel.send(message);
  appendChat('local', message);
  appendLog(el['pc-log'], `data channel -> ${message}`);
}

function toggleTrack(kind) {
  if (!state.localStream) {
    notify('warn', 'Start camera first.');
    return;
  }
  const tracks = kind === 'audio' ? state.localStream.getAudioTracks() : state.localStream.getVideoTracks();
  if (!tracks.length) return;
  const nextEnabled = !tracks[0].enabled;
  tracks.forEach(track => { track.enabled = nextEnabled; });
  if (kind === 'audio') {
    state.audioEnabled = nextEnabled;
    document.getElementById('toggle-audio').textContent = nextEnabled ? 'Mute audio' : 'Unmute audio';
  } else {
    state.videoEnabled = nextEnabled;
    document.getElementById('toggle-video').textContent = nextEnabled ? 'Hide video' : 'Show video';
  }
  setText(el['local-label'], `${state.videoEnabled ? 'camera' : 'video-off'} / ${state.audioEnabled ? 'mic' : 'audio-off'}`);
}

function clearNotifications() {
  el.notifications.innerHTML = '<div class="notification muted">No live events yet.</div>';
}

if (realtimePage) {
  document.getElementById('refresh-runtime').addEventListener('click', () => refreshRuntime().catch(console.error));
  document.getElementById('session-refresh').addEventListener('click', () => refreshRuntime().catch(console.error));
  document.getElementById('csrf-refresh').addEventListener('click', () => refreshCsrf().catch(console.error));
  document.getElementById('meeting-create').addEventListener('click', createMeeting);
  document.getElementById('meeting-join').addEventListener('click', joinMeeting);
  document.getElementById('copy-invite').addEventListener('click', () => copyInvite().catch(console.error));
  document.getElementById('display-name').addEventListener('input', syncMeetingState);
  document.getElementById('meeting-id').addEventListener('input', syncMeetingState);
  document.getElementById('peer-register').addEventListener('click', () => registerPeer().catch(error => appendLog(el['signal-log'], error.message)));
  document.getElementById('peer-autopoll').addEventListener('click', togglePolling);
  document.getElementById('pc-offer').addEventListener('click', () => createOffer().catch(error => appendLog(el['pc-log'], error.message)));
  document.getElementById('call-hangup').addEventListener('click', () => hangupCall().catch(error => appendLog(el['pc-log'], error.message)));
  document.getElementById('pc-reset').addEventListener('click', resetPeerConnection);
  document.getElementById('media-start').addEventListener('click', () => ensureLocalMedia().catch(error => notify('error', `Media error: ${error.message}`)));
  document.getElementById('toggle-audio').addEventListener('click', () => toggleTrack('audio'));
  document.getElementById('toggle-video').addEventListener('click', () => toggleTrack('video'));
  document.getElementById('dc-send').addEventListener('click', sendDataChannelMessage);
  document.getElementById('ws-connect').addEventListener('click', connectWebSocket);
  document.getElementById('ws-send').addEventListener('click', sendWebSocketMessage);
  document.getElementById('clear-notifications').addEventListener('click', clearNotifications);

  consumeUrlState();
  createPeerConnection();
  refreshRuntime().catch(error => appendLog(el['runtime-json'], error.message));
  refreshCsrf().catch(error => appendLog(el['signal-log'], error.message));
}
