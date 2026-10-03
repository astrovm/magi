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

const pick = (items) => items[Math.floor(Math.random() * items.length)];

// Show the real host in the alias prefix (handy on preview deploys).
document.querySelectorAll('[data-host]').forEach((prefix) => {
  prefix.textContent = `${location.host}/`;
});

// Forms talk to the orb without leaving the page.
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
      if (response.ok && form.hasAttribute('data-reset')) {
        form.reset();
        wormRain(8);
      }
    } catch {
      output.innerHTML = '<p class="orb-msg orb-msg--error">the orb lost signal. try again.</p>';
    } finally {
      form.querySelectorAll('button').forEach((each) => { each.disabled = false; });
      if (button) {
        button.textContent = label;
      }
    }
  });
});

// Copy buttons, including the ones the orb sends back later.
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

// Poke the worm.
const worm = document.getElementById('worm');
const bubble = document.getElementById('bubble');
let bubbleTimer;
worm?.addEventListener('click', () => {
  bubble.textContent = pick(WORM_LINES.filter((line) => line !== bubble.textContent));
  bubble.hidden = false;
  worm.classList.remove('wiggle');
  void worm.offsetWidth;
  worm.classList.add('wiggle');
  clearTimeout(bubbleTimer);
  bubbleTimer = setTimeout(() => { bubble.hidden = true; }, 4000);
});

const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

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

// ↑↑↓↓←→←→BA: party mode.
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
