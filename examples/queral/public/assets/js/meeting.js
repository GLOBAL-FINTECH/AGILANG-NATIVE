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
  meetingRefreshTimer: null,
  pc: null,
  localStream: null,
  remoteStream: null,
  dataChannel: null,
  pendingCandidates: [],
  socket: null,
  offerSent: false,
  localMediaMode: 'none',
  joinStage: 'idle',
  traceId: '',
};

function peerLabelFromPeerId(peerId) {
  return String(peerId || '').replace(/^meeting-[^-]+-/, '').replace(/-/g, ' ').trim();
}

function remoteIdentityFromMember(member) {
  if (!member) return { title: 'Waiting for participant', caption: 'Remote participant' };
  const title = member.name || peerLabelFromPeerId(member.peer_id) || member.email || 'Remote participant';
  const role = member.role === 'host' ? 'Host' : 'Participant';
  const caption = member.email ? `${role} · ${member.email}` : role;
  return { title, caption };
}

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

function trace(event, details = {}, tone = '') {
  const payload = { event, traceId: meetingState.traceId || 'pending', ...details };
  console.log('[QUERAL trace]', payload);
  addActivity(`${event} ${JSON.stringify(details)}`, tone);
}

function setJoinStage(stage, text, tone = '') {
  meetingState.joinStage = stage;
  addActivity(`[${stage}] ${text}`, tone);
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
  syncRoleLabels();
}

function syncRoleLabels() {
  q('meeting-id-label').textContent = meetingState.meetingId || 'Not created';
  q('participant-name').textContent = meetingState.auth?.user?.name || meetingState.auth?.user?.email || 'Guest';
  q('participant-role').textContent = meetingState.isHost ? 'Host' : 'Participant';
}

function inviteUrl() {
  const url = new URL(location.origin + '/');
  url.searchParams.set('meeting', meetingState.meetingId);
  if (meetingState.hostPeerId) url.searchParams.set('host', meetingState.hostPeerId);
  return url.toString();
}

function updateRemoteParticipant(peerId) {
  if (!peerId || peerId === meetingState.localPeerId) return;
  meetingState.targetPeerId = peerId;
  if (!meetingState.hostPeerId) meetingState.hostPeerId = peerId;
  q('remote-name').textContent = peerLabelFromPeerId(peerId) || 'Remote participant';
}

function syncMembersFromMeeting(meeting) {
  const members = Array.isArray(meeting?.members) ? meeting.members : [];
  const memberCount = Number(meeting?.member_count ?? members.length ?? 0);
  q('peer-count').textContent = String(memberCount);
  const remoteMember = members.find(member => member.peer_id && member.peer_id !== meetingState.localPeerId);
  if (remoteMember?.peer_id) {
    updateRemoteParticipant(remoteMember.peer_id);
    const identity = remoteIdentityFromMember(remoteMember);
    q('remote-name').textContent = identity.title;
    q('remote-caption').textContent = identity.caption;
    addActivity(`[members] remote participant ${remoteMember.peer_id} is present`);
  } else {
    q('remote-name').textContent = 'Waiting for participant';
    q('remote-caption').textContent = 'Remote participant';
  }
}

async function resolveMeeting() {
  setJoinStage('meeting', `Resolving meeting ${meetingState.meetingId}.`);
  if (!meetingState.meetingId) return null;
  const result = await api(`/api/webrtc/meeting?meeting_id=${encodeURIComponent(meetingState.meetingId)}`);
  if (!result.response.ok) {
    throw new Error(result.json?.error || `Meeting lookup failed (${result.response.status})`);
  }
  const hostPeerId = safeSegment(result.json?.host_peer_id);
  if (hostPeerId) {
    meetingState.hostPeerId = hostPeerId;
    if (meetingState.localPeerId && hostPeerId !== meetingState.localPeerId) {
      meetingState.isHost = false;
      updateRemoteParticipant(hostPeerId);
      syncRoleLabels();
      setJoinStage('meeting', `Role set to participant; host is ${hostPeerId}.`, 'success');
    } else if (meetingState.localPeerId && hostPeerId === meetingState.localPeerId) {
      meetingState.isHost = true;
      meetingState.targetPeerId = '';
      syncRoleLabels();
      setJoinStage('meeting', `Role confirmed as host for ${hostPeerId}.`, 'success');
    } else if (!meetingState.isHost) {
      updateRemoteParticipant(hostPeerId);
    }
    setJoinStage('meeting', `Resolved host peer ${hostPeerId}.`, 'success');
  }
  trace('meeting:resolved', {
    meetingId: meetingState.meetingId,
    hostPeerId: meetingState.hostPeerId,
    targetPeerId: meetingState.targetPeerId,
    memberCount: result.json?.member_count ?? null,
  });
  syncMembersFromMeeting(result.json);
  return result.json;
}

async function maybeStartGuestOffer() {
  if (meetingState.isHost || !meetingState.registered || meetingState.offerSent) return;
  if (!meetingState.targetPeerId) {
    await resolveMeeting();
  }
  if (!meetingState.targetPeerId) return;
  await createOffer();
}

function scheduleMeetingRefresh(delay = 2000) {
  clearTimeout(meetingState.meetingRefreshTimer);
  meetingState.meetingRefreshTimer = setTimeout(async () => {
    try {
      await resolveMeeting();
      if (meetingState.registered && !meetingState.isHost) {
        await maybeStartGuestOffer();
      }
    } catch (error) {
      addActivity(error.message, 'error');
    } finally {
      if (meetingState.meetingId) {
        scheduleMeetingRefresh(meetingState.registered ? 1500 : 3000);
      }
    }
  }, delay);
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
  setJoinStage('media', 'Opening local camera and microphone.');
  const attempts = [
    { constraints: { audio: true, video: true }, mode: 'av', label: 'camera and microphone' },
    { constraints: { audio: true, video: false }, mode: 'audio', label: 'microphone only' },
    { constraints: { audio: false, video: true }, mode: 'video', label: 'camera only' },
  ];
  const failures = [];
  for (const attempt of attempts) {
    try {
      meetingState.localStream = await navigator.mediaDevices.getUserMedia(attempt.constraints);
      meetingState.localMediaMode = attempt.mode;
      q('local-video').srcObject = meetingState.localStream;
      refreshLocalPresentation();
      attachTracks();
      setJoinStage('media', `Local media ready in ${attempt.mode} mode.`, 'success');
      addActivity(`Local media ready: ${attempt.label}.`, 'success');
      return meetingState.localStream;
    } catch (error) {
      failures.push(`${attempt.label}: ${formatMediaError(error)}`);
    }
  }
  meetingState.localMediaMode = 'none';
  const message = `Media unavailable. ${failures[0] || 'No microphone or camera source could be opened.'}`;
  setJoinStage('media', message, 'warn');
  addActivity(message, 'warn');
  refreshLocalPresentation();
  return null;
}

function formatMediaError(error) {
  if (!error) return 'Unknown media error';
  const name = error.name || 'MediaError';
  const message = error.message || 'Unable to access the requested device.';
  return `${name}: ${message}`;
}

function streamHasEnabledVideo(stream) {
  return Boolean(stream?.getVideoTracks().some(track => track.readyState === 'live' && track.enabled));
}

function streamHasLiveAudio(stream) {
  return Boolean(stream?.getAudioTracks().some(track => track.readyState === 'live' && track.enabled));
}

function refreshLocalPresentation() {
  const hasVideo = streamHasEnabledVideo(meetingState.localStream);
  const hasAudio = streamHasLiveAudio(meetingState.localStream);
  q('local-placeholder').hidden = hasVideo;
  q('local-video').style.visibility = hasVideo ? 'visible' : 'hidden';
  q('local-video').style.opacity = hasVideo ? '1' : '0';
  q('local-video').muted = true;
  q('local-video').playsInline = true;
  const badge = hasVideo ? 'Local' : (hasAudio ? 'Audio only' : 'No media');
  q('local-media-badge').textContent = badge;
}

function refreshRemotePresentation() {
  const hasVideo = streamHasEnabledVideo(meetingState.remoteStream);
  const hasAudio = streamHasLiveAudio(meetingState.remoteStream);
  q('remote-placeholder').hidden = hasVideo;
  q('remote-video').style.visibility = hasVideo ? 'visible' : 'hidden';
  q('remote-video').style.opacity = hasVideo ? '1' : '0';
  const badge = hasVideo ? 'Remote' : (hasAudio ? 'Audio only' : 'Waiting');
  q('remote-media-badge').textContent = badge;
}

function createPeerConnection() {
  if (meetingState.pc && meetingState.pc.signalingState !== 'closed') return meetingState.pc;
  setJoinStage('peer', 'Creating RTCPeerConnection.');
  const pc = new RTCPeerConnection({
    iceServers: [
      { urls: 'stun:stun.l.google.com:19302' },
      { urls: 'stun:stun1.l.google.com:19302' },
    ],
    iceCandidatePoolSize: 4,
  });
  meetingState.remoteStream = new MediaStream();
  q('remote-video').srcObject = meetingState.remoteStream;
  refreshRemotePresentation();
  trace('peer:create', {
    meetingId: meetingState.meetingId,
    localPeerId: meetingState.localPeerId,
    targetPeerId: meetingState.targetPeerId,
    isHost: meetingState.isHost,
  });

  pc.onicecandidate = event => {
    if (event.candidate && meetingState.targetPeerId) {
      addActivity(`[ice] candidate queued for ${meetingState.targetPeerId}`);
      trace('ice:local-candidate', {
        targetPeerId: meetingState.targetPeerId,
        candidateMid: event.candidate.sdpMid || '',
        candidateMLineIndex: event.candidate.sdpMLineIndex ?? null,
      });
      sendSignal('ice-candidate', JSON.stringify(event.candidate)).catch(error => addActivity(error.message, 'error'));
    } else if (!event.candidate) {
      trace('ice:gathering-complete', { peer: meetingState.localPeerId });
    }
  };
  pc.ontrack = event => {
    const tracks = event.streams[0]?.getTracks() || [event.track];
    for (const track of tracks) {
      if (!meetingState.remoteStream.getTracks().some(existing => existing.id === track.id)) {
        meetingState.remoteStream.addTrack(track);
      }
    }
    refreshRemotePresentation();
    setStatus('Connected', 'success');
    setJoinStage('track', 'Remote media track attached.', 'success');
    addActivity(streamHasEnabledVideo(meetingState.remoteStream) ? 'Remote video and audio connected.' : 'Remote audio connected; video not present.', streamHasEnabledVideo(meetingState.remoteStream) ? 'success' : 'warn');
    trace('track:remote-attached', {
      streamCount: event.streams.length,
      trackIds: tracks.map(track => track.id),
      trackKinds: tracks.map(track => track.kind),
    }, 'success');
  };
  pc.ondatachannel = event => {
    trace('datachannel:incoming', { label: event.channel.label });
    bindDataChannel(event.channel);
  };
  pc.onconnectionstatechange = () => {
    const state = pc.connectionState;
    q('connection-label').textContent = state;
    addActivity(`[connection] ${state}`);
    trace('peer:connection-state', { state });
    if (state === 'connected') setStatus('Meeting live', 'success');
    if (state === 'failed' || state === 'disconnected') {
      setStatus('Reconnecting…', 'warn');
      addActivity(`Connection ${state}; recovery started.`, 'warn');
      if (state === 'failed') pc.restartIce();
    }
  };
  pc.oniceconnectionstatechange = () => {
    q('ice-label').textContent = pc.iceConnectionState;
    trace('peer:ice-state', { state: pc.iceConnectionState });
  };
  pc.onsignalingstatechange = () => {
    q('signal-label').textContent = pc.signalingState;
    trace('peer:signaling-state', { state: pc.signalingState });
  };
  meetingState.pc = pc;
  attachTracks();
  return pc;
}

function attachTracks() {
  if (!meetingState.pc || !meetingState.localStream) return;
  const existing = new Set(meetingState.pc.getSenders().map(sender => sender.track?.id).filter(Boolean));
  for (const track of meetingState.localStream.getTracks()) {
    if (!existing.has(track.id)) {
      meetingState.pc.addTrack(track, meetingState.localStream);
      trace('track:local-attached', { trackId: track.id, kind: track.kind });
    }
  }
  refreshLocalPresentation();
}

function bindDataChannel(channel) {
  meetingState.dataChannel = channel;
  q('chat-state').textContent = channel.readyState;
  trace('datachannel:bind', { label: channel.label, state: channel.readyState });
  channel.onopen = () => {
    q('chat-state').textContent = 'live';
    addActivity('Live meeting chat connected.', 'success');
    trace('datachannel:open', { label: channel.label }, 'success');
  };
  channel.onclose = () => {
    q('chat-state').textContent = 'offline';
    trace('datachannel:close', { label: channel.label }, 'warn');
  };
  channel.onmessage = event => {
    let payload = { text: event.data, sender: 'Participant' };
    try { payload = JSON.parse(event.data); } catch (_) {}
    trace('datachannel:message', { sender: payload.sender || 'Participant', size: String(event.data || '').length });
    addMessage('remote', payload.text || event.data, payload.sender || 'Participant');
  };
}

async function registerPeer() {
  setJoinStage('register', `Registering peer ${meetingState.localPeerId}.`);
  const result = await api('/api/webrtc/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': meetingState.csrf },
    body: JSON.stringify({
      peer_id: meetingState.localPeerId,
      meeting_id: meetingState.meetingId,
      role: meetingState.isHost ? 'host' : 'participant',
    }),
  });
  if (!result.response.ok && result.response.status !== 409) {
    throw new Error(result.json?.error || `Peer registration failed (${result.response.status})`);
  }
  meetingState.registered = true;
  q('peer-count').textContent = String(result.json?.meeting?.member_count ?? result.json?.active_peers ?? '—');
  const hostPeerId = safeSegment(result.json?.meeting?.host_peer_id);
  if (hostPeerId) {
    meetingState.hostPeerId = hostPeerId;
    if (!meetingState.isHost) updateRemoteParticipant(hostPeerId);
  }
  syncMembersFromMeeting(result.json?.meeting);
  trace('peer:registered', {
    localPeerId: meetingState.localPeerId,
    hostPeerId,
    memberCount: result.json?.meeting?.member_count ?? null,
    activePeers: result.json?.active_peers ?? null,
    reused: result.response.status === 409,
  }, 'success');
  setJoinStage('register', `Peer registered. Active members: ${q('peer-count').textContent}.`, 'success');
  addActivity('Secure meeting identity registered.', 'success');
}

async function sendSignal(kind, payload) {
  if (!meetingState.targetPeerId) throw new Error('Remote participant is not known yet.');
  addActivity(`[signal:${kind}] ${meetingState.localPeerId} -> ${meetingState.targetPeerId}`);
  trace('signal:send', {
    kind,
    from: meetingState.localPeerId,
    to: meetingState.targetPeerId,
    payloadSize: String(payload || '').length,
  });
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
  trace('signal:queued', {
    kind,
    from: meetingState.localPeerId,
    to: meetingState.targetPeerId,
    status: result.response.status,
  }, 'success');
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
      addActivity(`[poll] received ${message.kind} from ${message.from}`);
      trace('signal:received', {
        kind: message.kind,
        from: message.from,
        to: message.to,
        payloadSize: String(message.payload || '').length,
      }, 'success');
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
    updateRemoteParticipant(message.from);
  }
  const pc = createPeerConnection();
  if (message.kind === 'offer') {
    setJoinStage('offer', `Received offer from ${message.from}.`);
    trace('offer:received', { from: message.from, signalingState: pc.signalingState }, 'success');
    await startLocalMedia();
    await pc.setRemoteDescription({ type: 'offer', sdp: message.payload });
    trace('offer:remote-description-set', { from: message.from, signalingState: pc.signalingState }, 'success');
    await flushCandidates();
    const answer = await pc.createAnswer();
    trace('answer:created', { to: message.from, sdpLength: String(answer.sdp || '').length });
    await pc.setLocalDescription(answer);
    trace('answer:local-description-set', { to: message.from, signalingState: pc.signalingState });
    await sendSignal('answer', answer.sdp || '');
    setJoinStage('answer', `Answer sent to ${message.from}.`, 'success');
    addActivity('Incoming call accepted automatically.', 'success');
  } else if (message.kind === 'answer') {
    setJoinStage('answer', `Received answer from ${message.from}.`, 'success');
    trace('answer:received', { from: message.from, signalingState: pc.signalingState }, 'success');
    await pc.setRemoteDescription({ type: 'answer', sdp: message.payload });
    trace('answer:remote-description-set', { from: message.from, signalingState: pc.signalingState }, 'success');
    await flushCandidates();
  } else if (message.kind === 'ice-candidate') {
    const candidate = JSON.parse(message.payload);
    if (pc.remoteDescription) {
      await pc.addIceCandidate(candidate);
      trace('ice:remote-candidate-applied', {
        from: message.from,
        candidateMid: candidate.sdpMid || '',
        candidateMLineIndex: candidate.sdpMLineIndex ?? null,
      }, 'success');
    } else {
      meetingState.pendingCandidates.push(candidate);
      trace('ice:remote-candidate-buffered', {
        from: message.from,
        buffered: meetingState.pendingCandidates.length,
      });
    }
  } else if (message.kind === 'hangup') {
    endPeerConnection(false);
    addActivity('The remote participant left the meeting.', 'warn');
    trace('peer:hangup-received', { from: message.from }, 'warn');
  }
}

async function flushCandidates() {
  while (meetingState.pendingCandidates.length && meetingState.pc?.remoteDescription) {
    const candidate = meetingState.pendingCandidates.shift();
    await meetingState.pc.addIceCandidate(candidate);
    trace('ice:buffered-candidate-applied', {
      remaining: meetingState.pendingCandidates.length,
      candidateMid: candidate?.sdpMid || '',
      candidateMLineIndex: candidate?.sdpMLineIndex ?? null,
    }, 'success');
  }
}

async function createOffer() {
  if (meetingState.offerSent) return;
  const pc = createPeerConnection();
  if (!meetingState.dataChannel) bindDataChannel(pc.createDataChannel('queral-chat', { ordered: true }));
  const offer = await pc.createOffer({ offerToReceiveAudio: true, offerToReceiveVideo: true });
  trace('offer:created', { to: meetingState.targetPeerId, sdpLength: String(offer.sdp || '').length });
  await pc.setLocalDescription(offer);
  trace('offer:local-description-set', { to: meetingState.targetPeerId, signalingState: pc.signalingState });
  await sendSignal('offer', offer.sdp || '');
  meetingState.offerSent = true;
  setJoinStage('offer', `Offer sent to ${meetingState.targetPeerId}.`, 'success');
  setStatus('Calling participant…', 'ready');
  addActivity('Secure call invitation sent.', 'success');
}

async function enterMeeting() {
  if (meetingState.joining) return;
  meetingState.joining = true;
  q('join-now').disabled = true;
  try {
    meetingState.traceId = `${meetingState.meetingId || 'meeting'}-${Date.now().toString(36)}`;
    trace('meeting:join-start', {
      meetingId: meetingState.meetingId,
      localPeerId: meetingState.localPeerId,
      hostPeerId: meetingState.hostPeerId,
      isHost: meetingState.isHost,
    });
    setJoinStage('join', 'Entering meeting.');
    setStatus('Preparing secure room…', 'ready');
    createPeerConnection();
    await registerPeer();
    setJoinStage('poll', 'Starting signaling poll loop.');
    pollOnce();
    setJoinStage('socket', 'Connecting presence socket.');
    connectActivitySocket();
    scheduleMeetingRefresh(500);
    await startLocalMedia();
    q('prejoin-screen').hidden = true;
    q('live-room').hidden = false;
    q('invite-link').value = inviteUrl();
    setStatus(
      meetingState.isHost
        ? (meetingState.localMediaMode === 'none' ? 'Waiting for participants without local media' : 'Waiting for participants')
        : (meetingState.localMediaMode === 'none' ? 'Connecting without local media…' : 'Connecting to host…'),
      meetingState.localMediaMode === 'none' ? 'warn' : 'ready',
    );
    if (!meetingState.isHost) await maybeStartGuestOffer();
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
    setJoinStage('socket', 'Presence socket connected.', 'success');
    trace('socket:open', { meetingId: meetingState.meetingId, localPeerId: meetingState.localPeerId }, 'success');
    socket.send(JSON.stringify({ type: 'presence', meeting: meetingState.meetingId, peer: meetingState.localPeerId }));
  };
  socket.onmessage = event => {
    addActivity(`Realtime event: ${event.data}`);
    try {
      const payload = JSON.parse(event.data);
      if (payload.type === 'presence' && payload.meeting === meetingState.meetingId && payload.peer && payload.peer !== meetingState.localPeerId) {
        updateRemoteParticipant(payload.peer);
        q('peer-count').textContent = String(Math.max(Number(q('peer-count').textContent || 1), 2));
        trace('socket:presence', { peer: payload.peer, meetingId: payload.meeting }, 'success');
        maybeStartGuestOffer().catch(error => addActivity(error.message, 'error'));
        scheduleMeetingRefresh(250);
      }
    } catch (_) {}
  };
  socket.onclose = () => {
    q('socket-label').textContent = 'reconnecting';
    addActivity('[socket] reconnecting');
    trace('socket:close', { meetingId: meetingState.meetingId }, 'warn');
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

async function toggleMedia(kind) {
  if (!meetingState.localStream) {
    await startLocalMedia();
    if (!meetingState.localStream) return;
  }
  const tracks = kind === 'audio' ? meetingState.localStream?.getAudioTracks() : meetingState.localStream?.getVideoTracks();
  if (!tracks?.length) return;
  const enabled = !tracks[0].enabled;
  tracks.forEach(track => { track.enabled = enabled; });
  const button = q(kind === 'audio' ? 'toggle-audio' : 'toggle-video');
  button.classList.toggle('off', !enabled);
  button.querySelector('span').textContent = kind === 'audio' ? (enabled ? 'Mute' : 'Unmute') : (enabled ? 'Stop video' : 'Start video');
  if (kind === 'video') refreshLocalPresentation();
}

function endPeerConnection(notifyRemote = true) {
  if (notifyRemote && meetingState.targetPeerId) sendSignal('hangup', 'hangup').catch(() => {});
  try { meetingState.dataChannel?.close(); } catch (_) {}
  try { meetingState.pc?.close(); } catch (_) {}
  try { meetingState.remoteStream?.getTracks().forEach(track => track.stop()); } catch (_) {}
  meetingState.pc = null;
  meetingState.dataChannel = null;
  meetingState.remoteStream = null;
  meetingState.offerSent = false;
  meetingState.pendingCandidates = [];
  meetingState.targetPeerId = meetingState.isHost ? '' : meetingState.hostPeerId;
  clearTimeout(meetingState.meetingRefreshTimer);
  q('remote-placeholder').hidden = false;
  q('remote-video').srcObject = null;
  q('remote-video').style.visibility = 'hidden';
  q('remote-video').style.opacity = '0';
  q('remote-name').textContent = 'Waiting for participant';
  q('remote-media-badge').textContent = 'Waiting';
  q('local-media-badge').textContent = streamHasEnabledVideo(meetingState.localStream) ? 'Local' : (streamHasLiveAudio(meetingState.localStream) ? 'Audio only' : 'No media');
  q('chat-state').textContent = 'offline';
  q('connection-label').textContent = 'idle';
  q('ice-label').textContent = 'idle';
  setStatus('Call ended', 'warn');
  if (meetingState.meetingId) {
    scheduleMeetingRefresh(1000);
  }
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
  scheduleMeetingRefresh(1000);
  if (!meetingState.isHost) {
    resolveMeeting().catch(() => {});
  }
}

q('new-meeting').addEventListener('click', createNewMeeting);
q('join-meeting').addEventListener('click', openMeetingFromCode);
q('join-now').addEventListener('click', enterMeeting);
q('copy-invite').addEventListener('click', () => copyInvite().catch(error => addActivity(error.message, 'error')));
q('toggle-audio').addEventListener('click', () => toggleMedia('audio').catch(error => addActivity(error.message, 'error')));
q('toggle-video').addEventListener('click', () => toggleMedia('video').catch(error => addActivity(error.message, 'error')));
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
