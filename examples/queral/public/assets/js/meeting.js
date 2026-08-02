const q = id => document.getElementById(id);

const meetingState = {
  auth: null,
  csrf: '',
  meetingId: '',
  hostPeerId: '',
  localPeerId: '',
  targetPeerId: '',
  isHost: false,
  registered: false,
  joining: false,
  polling: false,
  pollDelay: 350,
  pollTimer: null,
  pc: null,
  localStream: null,
  remoteStream: null,
  dataChannel: null,
  pendingCandidates: [],
  socket: null,
};

function safeSegment(value) {
  return String(value || '').trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 56);
}

function setStatus(text, tone = '') {
  q('meeting-status').textContent = text;
  q('meeting-status').dataset.tone = tone;
}

function addActivity(text, tone = '') {
  const list = q('activity-list');
  const item = document.createElement('div');
  item.className = `activity-item ${tone}`;
  item.textContent = `${new Date().toLocaleTimeString()}  ${text}`;
  list.prepend(item);
  while (list.children.length > 30) list.lastElementChild.remove();
}

function addMessage(side, text, sender = '') {
  const list = q('chat-list');
  const row = document.createElement('div');
  row.className = `message ${side}`;
  if (sender) {
    const label = document.createElement('span');
    label.className = 'message-sender';
    label.textContent = sender;
    row.appendChild(label);
  }
  const body = document.createElement('span');
  body.textContent = text;
  row.appendChild(body);
  list.appendChild(row);
  list.scrollTop = list.scrollHeight;
}

async function api(url, options = {}) {
  const response = await fetch(url, {
    credentials: 'same-origin',
    ...options,
    headers: { Accept: 'application/json', ...(options.headers || {}) },
  });
  const text = await response.text();
  let json = null;
  try { json = text ? JSON.parse(text) : null; } catch (_) { json = { raw: text }; }
  return { response, json };
}

function readUrl() {
  const url = new URL(location.href);
  meetingState.meetingId = safeSegment(url.searchParams.get('meeting'));
  meetingState.hostPeerId = safeSegment(url.searchParams.get('host'));
  meetingState.isHost = !meetingState.hostPeerId;
  q('meeting-code').value = meetingState.meetingId;
}

function derivePeerIdentity() {
  const identity = safeSegment(meetingState.auth?.user?.email || meetingState.auth?.user?.id || 'guest');
  meetingState.localPeerId = `meeting-${meetingState.meetingId}-${identity}`;
  if (meetingState.isHost) {
    meetingState.hostPeerId = meetingState.localPeerId;
    meetingState.targetPeerId = '';
  } else {
    meetingState.targetPeerId = meetingState.hostPeerId;
  }
  q('meeting-id-label').textContent = meetingState.meetingId || 'Not created';
  q('participant-name').textContent = meetingState.auth?.user?.name || meetingState.auth?.user?.email || 'Guest';
  q('participant-role').textContent = meetingState.isHost ? 'Host' : 'Participant';
}

function inviteUrl() {
  const url = new URL(location.origin + '/');
  url.searchParams.set('meeting', meetingState.meetingId);
  url.searchParams.set('host', meetingState.hostPeerId);
  return url.toString();
}

async function loadAuthAndCsrf() {
  const auth = await api('/api/auth/me');
  if (auth.response.status === 401) {
    const returnTo = `${location.pathname}${location.search}${location.hash}`;
    location.replace(`/login?return_to=${encodeURIComponent(returnTo)}`);
    return false;
  }
  meetingState.auth = auth.json;
  const csrfPage = await fetch('/dashboard', { credentials: 'same-origin' });
  const html = await csrfPage.text();
  meetingState.csrf = html.match(/name="_csrf"\s+value="([^"]+)"/)?.[1] || '';
  if (!meetingState.csrf) throw new Error('Unable to establish secure meeting session.');
  return true;
}

function createNewMeeting() {
  meetingState.meetingId = String(Math.floor(100000000 + Math.random() * 900000000));
  meetingState.isHost = true;
  derivePeerIdentity();
  const url = new URL(location.href);
  url.searchParams.set('meeting', meetingState.meetingId);
  url.searchParams.delete('host');
  history.replaceState({}, '', url);
  q('meeting-code').value = meetingState.meetingId;
  q('invite-link').value = inviteUrl();
  q('welcome-screen').hidden = true;
  q('meeting-screen').hidden = false;
  setStatus('Ready to start', 'ready');
}

function openMeetingFromCode() {
  const code = safeSegment(q('meeting-code').value);
  if (!code) return;
  const url = new URL(location.href);
  url.searchParams.set('meeting', code);
  location.href = url.toString();
}

async function startLocalMedia() {
  if (meetingState.localStream) return meetingState.localStream;
  meetingState.localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
  q('local-video').srcObject = meetingState.localStream;
  q('local-placeholder').hidden = true;
  attachTracks();
  addActivity('Camera and microphone are ready.', 'success');
  return meetingState.localStream;
}

function createPeerConnection() {
  if (meetingState.pc && meetingState.pc.signalingState !== 'closed') return meetingState.pc;
  const pc = new RTCPeerConnection({
    iceServers: [
      { urls: 'stun:stun.l.google.com:19302' },
      { urls: 'stun:stun1.l.google.com:19302' },
    ],
    iceCandidatePoolSize: 4,
  });
  meetingState.remoteStream = new MediaStream();
  q('remote-video').srcObject = meetingState.remoteStream;

  pc.onicecandidate = event => {
    if (event.candidate && meetingState.targetPeerId) {
      sendSignal('ice-candidate', JSON.stringify(event.candidate)).catch(error => addActivity(error.message, 'error'));
    }
  };
  pc.ontrack = event => {
    const tracks = event.streams[0]?.getTracks() || [event.track];
    for (const track of tracks) {
      if (!meetingState.remoteStream.getTracks().some(existing => existing.id === track.id)) {
        meetingState.remoteStream.addTrack(track);
      }
    }
    q('remote-placeholder').hidden = true;
    setStatus('Connected', 'success');
    addActivity('Remote video and audio connected.', 'success');
  };
  pc.ondatachannel = event => bindDataChannel(event.channel);
  pc.onconnectionstatechange = () => {
    const state = pc.connectionState;
    q('connection-label').textContent = state;
    if (state === 'connected') setStatus('Meeting live', 'success');
    if (state === 'failed' || state === 'disconnected') {
      setStatus('Reconnecting…', 'warn');
      addActivity(`Connection ${state}; recovery started.`, 'warn');
      if (state === 'failed') pc.restartIce();
    }
  };
  pc.oniceconnectionstatechange = () => { q('ice-label').textContent = pc.iceConnectionState; };
  pc.onsignalingstatechange = () => { q('signal-label').textContent = pc.signalingState; };
  meetingState.pc = pc;
  attachTracks();
  return pc;
}

function attachTracks() {
  if (!meetingState.pc || !meetingState.localStream) return;
  const existing = new Set(meetingState.pc.getSenders().map(sender => sender.track?.id).filter(Boolean));
  for (const track of meetingState.localStream.getTracks()) {
    if (!existing.has(track.id)) meetingState.pc.addTrack(track, meetingState.localStream);
  }
}

function bindDataChannel(channel) {
  meetingState.dataChannel = channel;
  q('chat-state').textContent = channel.readyState;
  channel.onopen = () => {
    q('chat-state').textContent = 'live';
    addActivity('Live meeting chat connected.', 'success');
  };
  channel.onclose = () => { q('chat-state').textContent = 'offline'; };
  channel.onmessage = event => {
    let payload = { text: event.data, sender: 'Participant' };
    try { payload = JSON.parse(event.data); } catch (_) {}
    addMessage('remote', payload.text || event.data, payload.sender || 'Participant');
  };
}

async function registerPeer() {
  const result = await api('/api/webrtc/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': meetingState.csrf },
    body: JSON.stringify({ peer_id: meetingState.localPeerId }),
  });
  if (!result.response.ok && result.response.status !== 409) {
    throw new Error(result.json?.error || `Peer registration failed (${result.response.status})`);
  }
  meetingState.registered = true;
  q('peer-count').textContent = String(result.json?.active_peers ?? '—');
  addActivity('Secure meeting identity registered.', 'success');
}

async function sendSignal(kind, payload) {
  if (!meetingState.targetPeerId) throw new Error('Remote participant is not known yet.');
  const result = await api('/api/webrtc/signal', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': meetingState.csrf },
    body: JSON.stringify({
      from: meetingState.localPeerId,
      to: meetingState.targetPeerId,
      kind,
      payload,
    }),
  });
  if (!result.response.ok) throw new Error(result.json?.error || `${kind} delivery failed (${result.response.status})`);
}

async function pollOnce() {
  if (!meetingState.registered || meetingState.polling) return;
  meetingState.polling = true;
  try {
    const result = await api(`/api/webrtc/poll?peer_id=${encodeURIComponent(meetingState.localPeerId)}`);
    if (result.response.status === 401) {
      location.replace(`/login?return_to=${encodeURIComponent(location.pathname + location.search)}`);
      return;
    }
    if (!result.response.ok) throw new Error(result.json?.error || `Signaling poll failed (${result.response.status})`);
    const message = result.json?.message;
    if (message) {
      meetingState.pollDelay = 80;
      await receiveSignal(message);
    } else {
      meetingState.pollDelay = Math.min(1100, meetingState.pollDelay + 100);
    }
  } catch (error) {
    meetingState.pollDelay = Math.min(3000, meetingState.pollDelay * 2);
    addActivity(error.message, 'error');
  } finally {
    meetingState.polling = false;
    meetingState.pollTimer = setTimeout(pollOnce, meetingState.pollDelay);
  }
}

async function receiveSignal(message) {
  if (message.from && message.from !== meetingState.localPeerId) {
    meetingState.targetPeerId = message.from;
    q('remote-name').textContent = message.from.replace(/^meeting-[^-]+-/, '').replace(/-/g, ' ');
  }
  const pc = createPeerConnection();
  if (message.kind === 'offer') {
    await startLocalMedia();
    await pc.setRemoteDescription({ type: 'offer', sdp: message.payload });
    await flushCandidates();
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    await sendSignal('answer', answer.sdp || '');
    addActivity('Incoming call accepted automatically.', 'success');
  } else if (message.kind === 'answer') {
    await pc.setRemoteDescription({ type: 'answer', sdp: message.payload });
    await flushCandidates();
  } else if (message.kind === 'ice-candidate') {
    const candidate = JSON.parse(message.payload);
    if (pc.remoteDescription) await pc.addIceCandidate(candidate);
    else meetingState.pendingCandidates.push(candidate);
  } else if (message.kind === 'hangup') {
    endPeerConnection(false);
    addActivity('The remote participant left the meeting.', 'warn');
  }
}

async function flushCandidates() {
  while (meetingState.pendingCandidates.length && meetingState.pc?.remoteDescription) {
    await meetingState.pc.addIceCandidate(meetingState.pendingCandidates.shift());
  }
}

async function createOffer() {
  const pc = createPeerConnection();
  if (!meetingState.dataChannel) bindDataChannel(pc.createDataChannel('queral-chat', { ordered: true }));
  const offer = await pc.createOffer({ offerToReceiveAudio: true, offerToReceiveVideo: true });
  await pc.setLocalDescription(offer);
  await sendSignal('offer', offer.sdp || '');
  setStatus('Calling participant…', 'ready');
  addActivity('Secure call invitation sent.', 'success');
}

async function enterMeeting() {
  if (meetingState.joining) return;
  meetingState.joining = true;
  q('join-now').disabled = true;
  try {
    setStatus('Preparing devices…', 'ready');
    await startLocalMedia();
    createPeerConnection();
    await registerPeer();
    pollOnce();
    connectActivitySocket();
    q('prejoin-screen').hidden = true;
    q('live-room').hidden = false;
    q('invite-link').value = inviteUrl();
    setStatus(meetingState.isHost ? 'Waiting for participants' : 'Connecting to host…', 'ready');
    if (!meetingState.isHost) await createOffer();
  } catch (error) {
    setStatus(error.message, 'error');
    addActivity(error.message, 'error');
    q('join-now').disabled = false;
  } finally {
    meetingState.joining = false;
  }
}

function connectActivitySocket() {
  if (meetingState.socket && meetingState.socket.readyState <= WebSocket.OPEN) return;
  const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const socket = new WebSocket(`${scheme}//${location.host}/ws`);
  meetingState.socket = socket;
  socket.onopen = () => {
    q('socket-label').textContent = 'live';
    socket.send(JSON.stringify({ type: 'presence', meeting: meetingState.meetingId, peer: meetingState.localPeerId }));
  };
  socket.onmessage = event => addActivity(`Realtime event: ${event.data}`);
  socket.onclose = () => {
    q('socket-label').textContent = 'reconnecting';
    setTimeout(connectActivitySocket, 1200);
  };
}

function sendChat() {
  const input = q('chat-input');
  const text = input.value.trim();
  if (!text) return;
  if (!meetingState.dataChannel || meetingState.dataChannel.readyState !== 'open') {
    addActivity('Chat will become available when the call connects.', 'warn');
    return;
  }
  const sender = meetingState.auth?.user?.name || meetingState.auth?.user?.email || 'You';
  meetingState.dataChannel.send(JSON.stringify({ type: 'chat', text, sender, sentAt: Date.now() }));
  addMessage('local', text, 'You');
  input.value = '';
}

function toggleMedia(kind) {
  const tracks = kind === 'audio' ? meetingState.localStream?.getAudioTracks() : meetingState.localStream?.getVideoTracks();
  if (!tracks?.length) return;
  const enabled = !tracks[0].enabled;
  tracks.forEach(track => { track.enabled = enabled; });
  const button = q(kind === 'audio' ? 'toggle-audio' : 'toggle-video');
  button.classList.toggle('off', !enabled);
  button.querySelector('span').textContent = kind === 'audio' ? (enabled ? 'Mute' : 'Unmute') : (enabled ? 'Stop video' : 'Start video');
}

function endPeerConnection(notifyRemote = true) {
  if (notifyRemote && meetingState.targetPeerId) sendSignal('hangup', 'hangup').catch(() => {});
  try { meetingState.dataChannel?.close(); } catch (_) {}
  try { meetingState.pc?.close(); } catch (_) {}
  meetingState.pc = null;
  meetingState.dataChannel = null;
  q('remote-placeholder').hidden = false;
  q('chat-state').textContent = 'offline';
  setStatus('Call ended', 'warn');
}

async function copyInvite() {
  const value = inviteUrl();
  q('invite-link').value = value;
  await navigator.clipboard.writeText(value);
  addActivity('Meeting invitation copied.', 'success');
}

async function bootstrap() {
  readUrl();
  if (!(await loadAuthAndCsrf())) return;
  if (!meetingState.meetingId) {
    q('welcome-screen').hidden = false;
    q('meeting-screen').hidden = true;
    return;
  }
  derivePeerIdentity();
  q('welcome-screen').hidden = true;
  q('meeting-screen').hidden = false;
  q('prejoin-screen').hidden = false;
  q('live-room').hidden = true;
  q('invite-link').value = inviteUrl();
  setStatus(meetingState.isHost ? 'Ready to start meeting' : 'Ready to join meeting', 'ready');
}

q('new-meeting').addEventListener('click', createNewMeeting);
q('join-meeting').addEventListener('click', openMeetingFromCode);
q('join-now').addEventListener('click', enterMeeting);
q('copy-invite').addEventListener('click', () => copyInvite().catch(error => addActivity(error.message, 'error')));
q('toggle-audio').addEventListener('click', () => toggleMedia('audio'));
q('toggle-video').addEventListener('click', () => toggleMedia('video'));
q('leave-call').addEventListener('click', () => endPeerConnection(true));
q('chat-send').addEventListener('click', sendChat);
q('chat-input').addEventListener('keydown', event => { if (event.key === 'Enter') sendChat(); });
q('toggle-sidebar').addEventListener('click', () => q('meeting-shell').classList.toggle('sidebar-closed'));
q('open-diagnostics').addEventListener('click', () => q('diagnostics').showModal());
q('close-diagnostics').addEventListener('click', () => q('diagnostics').close());

bootstrap().catch(error => {
  setStatus(error.message, 'error');
  addActivity(error.message, 'error');
});
