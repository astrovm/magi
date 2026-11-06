// The editor on a live paste's page. Built into public/live.js.
import { Compartment, EditorState } from '@codemirror/state';
import { basicSetup, EditorView } from 'codemirror';
import * as decoding from 'lib0/decoding';
import { yCollab } from 'y-codemirror.next';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

const NOTICE_MESSAGE = 4;
const NOTICE_LOCKED = 1;
const NOTICE_UNLOCKED = 2;
const NOTICE_FULL = 3;
const CLOSE_GONE = 4404;
const COLORS = ['#ff7cc0', '#ffd84d', '#7dffb0', '#7cc7ff', '#c79bff', '#ff9d6b'];
const MY_LINKS_KEY = 'magi:links';

const root = document.getElementById('live');
const { alias, role } = root.dataset;
const statusLine = document.getElementById('live-status');

const doc = new Y.Doc();
const text = doc.getText('codemirror');
const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
const provider = new WebsocketProvider(`${scheme}//${location.host}/paste/ws`, alias, doc);
const color = COLORS[Math.floor(Math.random() * COLORS.length)];

if (role === 'view') {
  provider.awareness.setLocalState(null);
} else {
  const wizard = Math.floor(Math.random() * 1000);
  provider.awareness.setLocalStateField('user', {
    name: `wizard ${wizard}`,
    color,
    colorLight: `${color}33`,
  });
}

let locked = false;
let full = false;
let connected = false;
let gone = false;

const editable = new Compartment();
const readOnly = () => role === 'view' || locked || full || gone;
const editableState = () => [
  EditorState.readOnly.of(readOnly()),
  EditorView.editable.of(!readOnly()),
];

const view = new EditorView({
  parent: document.getElementById('live-editor'),
  state: EditorState.create({
    doc: text.toString(),
    extensions: [
      basicSetup,
      EditorView.lineWrapping,
      EditorView.contentAttributes.of({ 'aria-label': `Text of ${alias}` }),
      EditorView.theme({
        '&': { height: '60vh', color: 'var(--ink)', backgroundColor: 'var(--field)', borderRadius: '14px' },
        '.cm-scroller': { fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' },
        '.cm-gutters': { backgroundColor: 'transparent', color: 'var(--ink-soft)', border: 'none' },
        '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'rgba(255, 255, 255, 0.06)' },
        '&.cm-focused': { outline: '3px solid var(--gold)' },
        '.cm-cursor': { borderLeftColor: 'var(--ink)' },
        '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: 'rgba(123, 63, 228, 0.55) !important' },
      }, { dark: true }),
      editable.of(editableState()),
      yCollab(text, provider.awareness, { undoManager: new Y.UndoManager(text) }),
    ],
  }),
});

function showStatus() {
  const others = Math.max(provider.awareness.getStates().size - (role === 'view' ? 0 : 1), 0);
  const company = others === 0 ? 'just you here' : `${others + 1} wizards here`;
  if (gone) {
    statusLine.textContent = '🌑 this paste is gone.';
  } else if (!connected) {
    statusLine.textContent = '📡 lost signal. your edits will sync when it comes back…';
  } else if (full) {
    statusLine.textContent = '🐘 this paste is full. reload to keep up with the others.';
  } else if (locked) {
    statusLine.textContent = `🔒 locked by the owner. ${company}.`;
  } else if (role === 'view') {
    statusLine.textContent = '👀 connected, view only.';
  } else {
    statusLine.textContent = `✨ connected. ${company}.`;
  }
}

function applyAccess() {
  view.dispatch({ effects: editable.reconfigure(editableState()) });
  showStatus();
  showOwnerTools();
}

provider.messageHandlers[NOTICE_MESSAGE] = (_encoder, decoder) => {
  const code = decoding.readVarUint(decoder);
  if (code === NOTICE_LOCKED) {
    locked = true;
  } else if (code === NOTICE_UNLOCKED) {
    locked = false;
  } else if (code === NOTICE_FULL) {
    full = true;
  }
  applyAccess();
};

provider.on('status', ({ status }) => {
  connected = status === 'connected';
  showStatus();
});
provider.on('connection-close', (event) => {
  if (event?.code === CLOSE_GONE) {
    gone = true;
    provider.disconnect();
    applyAccess();
  }
});
provider.awareness.on('change', showStatus);

/* owner tools: only for whoever has the manage key in this browser */

function savedLinks() {
  try {
    return JSON.parse(localStorage.getItem(MY_LINKS_KEY) ?? '[]');
  } catch {
    return [];
  }
}

const isThisPaste = (link) => link.alias.toLowerCase() === alias.toLowerCase();

function forgetThisPaste() {
  try {
    localStorage.setItem(MY_LINKS_KEY, JSON.stringify(savedLinks().filter((link) => !isThisPaste(link))));
  } catch {
    // Only "my links" would still show it, and that link answers 404 now.
  }
}

const owner = document.getElementById('live-owner');
const manageKey = role === 'edit' ? savedLinks().find(isThisPaste)?.key : undefined;

function showOwnerTools() {
  if (!owner || !manageKey) {
    return;
  }
  owner.hidden = gone;
  owner.querySelector('[data-owner="lock"]').hidden = locked;
  owner.querySelector('[data-owner="unlock"]').hidden = !locked;
}

async function manage(action, extra = {}) {
  const body = new FormData();
  Object.entries({ alias, key: manageKey, action, ...extra }).forEach(([name, value]) => body.set(name, value));
  return fetch('/manage', { method: 'POST', body, headers: { 'X-Orb': 'fragment' } });
}

function showOwnerResult(html) {
  document.getElementById('live-owner-result').innerHTML = html;
}

async function loadSnapshots() {
  const list = document.getElementById('live-snapshots');
  const response = await manage('snapshots');
  if (!response.ok) {
    return;
  }
  const snapshots = await response.json();
  list.replaceChildren(...snapshots.map((snapshot) => {
    const item = document.createElement('li');
    const label = document.createElement('span');
    label.textContent = `${new Date(snapshot.at).toLocaleString()}: ${snapshot.preview || '(empty)'}`;
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'chip';
    button.textContent = 'restore';
    button.addEventListener('click', async () => {
      showOwnerResult(await (await manage('restore', { snapshot: snapshot.id })).text());
      await loadSnapshots();
    });
    item.append(label, button);
    return item;
  }));
}

if (owner && manageKey) {
  showOwnerTools();
  owner.addEventListener('toggle', () => {
    if (owner.open) {
      loadSnapshots();
    }
  });
  owner.querySelectorAll('[data-owner]').forEach((button) => {
    button.addEventListener('click', async () => {
      const action = button.dataset.owner;
      if (action === 'banish' && !confirm('banish this paste forever? this deletes it for everyone.')) {
        return;
      }
      const response = await manage(action);
      showOwnerResult(await response.text());
      if (action === 'banish' && response.ok) {
        forgetThisPaste();
      }
    });
  });
}

showStatus();
