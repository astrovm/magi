'use strict';

const LOADING_LINES = [
  'consulting the orb…',
  'the worm is wiggling…',
  'bribing the wizard…',
  'asking the mushrooms…',
  'charging the orb…',
  'reticulating splines…',
];

const WORM_LINES = [
  'i am not a snake. please stop asking.',
  'every link i shorten makes me 1cm longer.',
  "don't ask what's in the orb.",
  'i went to wizard school. online. it counts.',
  'the hat is load-bearing.',
  'have you tried turning the orb off and on again?',
  "never trust a link that says 'free robux'.",
  'i have no hands. i type with my face.',
  'shhh. the orb is thinking.',
  '301 moved permanently. like me, from the dirt to the forest.',
  'curse a link. do it. nobody will know it was you.',
  'add a + to any link and the orb spills the tea.',
];

const CHEER_LINES = [
  'behold! a link!',
  'nailed it. no hands.',
  'the orb approves.',
  'another one for the collection.',
];

const SAD_LINES = [
  'the orb said no. i tried.',
  'oops. my hat slipped.',
  "that didn't go as planned.",
  'the mushrooms advised against it.',
];

const SLEEP_AFTER_MS = 60000;
const MY_LINKS_KEY = 'magi:links';
const MAX_SAVED_LINKS = 50;

const pick = (items) => items[Math.floor(Math.random() * items.length)];
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

// Show the real host in the alias prefix (handy on preview deploys).
document.querySelectorAll('[data-host]').forEach((prefix) => {
  prefix.textContent = `${location.host}/`;
});

/* the worm */

const worm = document.getElementById('worm');
const bubble = document.getElementById('bubble');
let bubbleTimer;
let sleepTimer;

function say(line) {
  if (!bubble) {
    return;
  }
  bubble.textContent = line;
  bubble.hidden = false;
  clearTimeout(bubbleTimer);
  bubbleTimer = setTimeout(() => { bubble.hidden = true; }, 4000);
}

function setMood(mood) {
  if (!worm) {
    return;
  }
  worm.classList.remove('wiggle', 'cheer', 'sad', 'sleepy');
  void worm.offsetWidth;
  if (mood) {
    worm.classList.add(mood);
  }
}

function wakeUp() {
  if (worm?.classList.contains('sleepy')) {
    setMood(null);
  }
  clearTimeout(sleepTimer);
  sleepTimer = setTimeout(() => {
    setMood('sleepy');
    say('zzz… wake me when you have a link.');
  }, SLEEP_AFTER_MS);
}

['pointerdown', 'keydown', 'input', 'scroll'].forEach((type) => {
  document.addEventListener(type, wakeUp, { passive: true });
});
wakeUp();

worm?.addEventListener('click', () => {
  say(pick(WORM_LINES.filter((line) => line !== bubble.textContent)));
  setMood('wiggle');
});

function wormRain(count) {
  if (reducedMotion.matches) {
    return;
  }
  for (let index = 0; index < count; index += 1) {
    const drop = document.createElement('span');
    drop.className = 'falling-worm';
    drop.textContent = pick(['🪱', '🔮', '✨', '🧙', '🍄', '⭐']);
    drop.style.left = `${Math.random() * 100}vw`;
    drop.style.animationDuration = `${2 + Math.random() * 2}s`;
    drop.style.animationDelay = `${Math.random() * 0.6}s`;
    drop.addEventListener('animationend', () => drop.remove());
    document.body.append(drop);
  }
}

/* my links: saved only in this browser */

const myLinks = document.getElementById('my-links');

function loadLinks() {
  try {
    const links = JSON.parse(localStorage.getItem(MY_LINKS_KEY) ?? '[]');
    return Array.isArray(links) ? links : [];
  } catch {
    return [];
  }
}

function saveLinks(links) {
  try {
    localStorage.setItem(MY_LINKS_KEY, JSON.stringify(links.slice(0, MAX_SAVED_LINKS)));
  } catch {
    // Private mode or full storage: the list just won't persist.
  }
  renderLinks();
}

function rememberLink({ alias, short, key }) {
  const links = loadLinks().filter((link) => link.alias.toLowerCase() !== alias.toLowerCase());
  saveLinks([{ alias, short, key, created: Date.now() }, ...links]);
}

function forgetLink(alias) {
  saveLinks(loadLinks().filter((link) => link.alias.toLowerCase() !== alias.toLowerCase()));
}

function chip(label, attributes) {
  const element = document.createElement(attributes.href ? 'a' : 'button');
  element.className = 'chip';
  element.textContent = label;
  Object.entries(attributes).forEach(([name, value]) => element.setAttribute(name, value));
  if (!attributes.href) {
    element.type = 'button';
  }
  return element;
}

function renderLinks() {
  if (!myLinks) {
    return;
  }
  const links = loadLinks();
  myLinks.hidden = links.length === 0;
  myLinks.querySelector('[data-count]').textContent = `(${links.length})`;
  const list = myLinks.querySelector('[data-list]');
  list.replaceChildren(...links.map((link) => {
    const item = document.createElement('li');
    const anchor = document.createElement('a');
    anchor.className = 'short-link';
    anchor.href = link.short;
    anchor.textContent = link.short.replace(/^https?:\/\//, '');
    item.append(
      anchor,
      chip('copy', { 'data-copy-link': link.short }),
      chip('peek', { href: `${link.short}+` }),
      chip('manage', { 'data-manage': link.alias }),
      chip('forget', { 'data-forget': link.alias }),
    );
    return item;
  }));
}

myLinks?.addEventListener('click', async (event) => {
  const target = event.target.closest('button');
  if (!target) {
    return;
  }
  if (target.dataset.copyLink) {
    await navigator.clipboard.writeText(target.dataset.copyLink);
    target.textContent = 'copied ✨';
    setTimeout(() => { target.textContent = 'copy'; }, 1500);
  }
  if (target.dataset.manage) {
    const link = loadLinks().find((each) => each.alias === target.dataset.manage);
    const manage = document.querySelector('details.manage');
    manage.open = true;
    document.getElementById('manage-alias').value = link.alias;
    document.getElementById('manage-key').value = link.key;
    document.getElementById('manage-url').focus();
    manage.scrollIntoView({ behavior: reducedMotion.matches ? 'auto' : 'smooth', block: 'center' });
  }
  if (target.dataset.forget && confirm('forget this link here? it keeps working, but you lose its manage key.')) {
    forgetLink(target.dataset.forget);
  }
});

renderLinks();

/* turnstile: only when the server has a site key */

let turnstileWidget = null;

async function setUpTurnstile() {
  const slot = document.getElementById('turnstile');
  if (!slot) {
    return;
  }
  try {
    const { turnstileSiteKey } = await (await fetch('/orb')).json();
    if (!turnstileSiteKey) {
      return;
    }
    window.onTurnstileLoad = () => {
      turnstileWidget = window.turnstile.render(slot, {
        sitekey: turnstileSiteKey,
        theme: 'dark',
        appearance: 'interaction-only',
      });
    };
    const script = document.createElement('script');
    script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?onload=onTurnstileLoad&render=explicit';
    script.async = true;
    document.head.append(script);
  } catch {
    // No settings, no widget. The server still decides.
  }
}

setUpTurnstile();

/* alias check while typing */

const aliasInput = document.getElementById('alias');
const aliasStatus = document.getElementById('alias-status');
const ALIAS_LABELS = { free: '✅ free', taken: '❌ taken', invalid: '❌ not allowed' };
let aliasTimer;
let aliasRequest = 0;

aliasInput?.addEventListener('input', () => {
  clearTimeout(aliasTimer);
  const value = aliasInput.value.trim();
  aliasStatus.textContent = '';
  delete aliasStatus.dataset.status;
  if (value === '') {
    return;
  }
  aliasTimer = setTimeout(async () => {
    const request = ++aliasRequest;
    try {
      const { status } = await (await fetch(`/orb?alias=${encodeURIComponent(value)}`)).json();
      if (request === aliasRequest) {
        aliasStatus.dataset.status = status;
        aliasStatus.textContent = ALIAS_LABELS[status];
      }
    } catch {
      // The real check happens on submit anyway.
    }
  }, 400);
});

/* forms talk to the orb without leaving the page */

document.querySelectorAll('form[data-orb]').forEach((form) => {
  const output = document.querySelector(form.dataset.orb);

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const button = event.submitter;
    if (button?.dataset.confirm && !confirm(button.dataset.confirm)) {
      return;
    }

    // Read the fields before disabling buttons, or the clicked action gets lost.
    const body = new FormData(form, button);
    const label = button?.textContent;
    form.querySelectorAll('button').forEach((each) => { each.disabled = true; });
    if (button) {
      button.textContent = pick(LOADING_LINES);
    }

    try {
      // getAttribute, because buttons named "action" shadow form.action.
      const response = await fetch(form.getAttribute('action'), {
        method: 'POST',
        body,
        headers: { 'X-Orb': 'fragment' },
      });
      output.innerHTML = await response.text();
      if (response.ok) {
        const summoned = output.querySelector('.summoned');
        if (summoned) {
          rememberLink(summoned.dataset);
        }
        if (body.get('action') === 'banish') {
          forgetLink(String(body.get('alias')));
        }
        if (form.hasAttribute('data-reset')) {
          form.reset();
          aliasStatus.textContent = '';
          wormRain(8);
        }
        setMood('cheer');
        say(pick(CHEER_LINES));
      } else {
        setMood('sad');
        say(pick(SAD_LINES));
      }
    } catch {
      output.innerHTML = '<p class="orb-msg orb-msg--error">the orb lost signal. try again.</p>';
      setMood('sad');
    } finally {
      form.querySelectorAll('button').forEach((each) => { each.disabled = false; });
      if (button) {
        button.textContent = label;
      }
      if (turnstileWidget !== null && form.contains(document.getElementById('turnstile'))) {
        window.turnstile.reset(turnstileWidget);
      }
    }
  });
});

/* copy buttons, including the ones the orb sends back later */

document.addEventListener('click', async (event) => {
  const button = event.target.closest('.copy-btn');
  if (!button) {
    return;
  }
  await navigator.clipboard.writeText(button.dataset.copy);
  button.textContent = 'copied ✨';
  button.classList.add('copied');
  setTimeout(() => {
    button.textContent = 'copy';
    button.classList.remove('copied');
  }, 1500);
});

/* countdown links */

const countdown = document.querySelector('.countdown');
if (countdown) {
  const number = countdown.querySelector('[data-countdown]');
  let left = Number(number.textContent);
  const tick = setInterval(() => {
    left -= 1;
    number.textContent = String(Math.max(left, 0));
    if (left <= 0) {
      clearInterval(tick);
      location.replace(countdown.dataset.target);
    }
  }, 1000);
}

/* seasons: ?season=halloween or ?season=winter to preview */

const SEASONS = {
  halloween: { months: [9], drops: ['🦇', '🍂', '👻'] },
  winter: { months: [11, 0], drops: ['❄️', '❄️', '✨'] },
};

function currentSeason() {
  const forced = new URLSearchParams(location.search).get('season');
  if (forced in SEASONS) {
    return forced;
  }
  const month = new Date().getMonth();
  return Object.keys(SEASONS).find((name) => SEASONS[name].months.includes(month));
}

const season = currentSeason();
if (season) {
  document.body.classList.add(season);
  const sky = document.querySelector('.season');
  if (sky && !reducedMotion.matches) {
    for (let index = 0; index < 10; index += 1) {
      const drop = document.createElement('span');
      drop.textContent = pick(SEASONS[season].drops);
      drop.style.left = `${Math.random() * 100}vw`;
      drop.style.animationDuration = `${8 + Math.random() * 8}s`;
      drop.style.animationDelay = `${-Math.random() * 16}s`;
      sky.append(drop);
    }
  }
}

/* ↑↑↓↓←→←→BA: party mode */

const KONAMI = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a'];
let konamiStep = 0;
document.addEventListener('keydown', (event) => {
  konamiStep = event.key === KONAMI[konamiStep] ? konamiStep + 1 : Number(event.key === KONAMI[0]);
  if (konamiStep === KONAMI.length) {
    konamiStep = 0;
    document.body.classList.toggle('party');
    wormRain(40);
  }
});
