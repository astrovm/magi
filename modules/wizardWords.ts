const ADJECTIVES = [
  'sneaky', 'wiggly', 'cursed', 'soggy', 'mystic', 'grumpy', 'sparkly', 'feral',
  'cosmic', 'crunchy', 'spooky', 'sleepy', 'chonky', 'goofy', 'shiny', 'damp',
  'ancient', 'smol', 'unhinged', 'polite', 'haunted', 'spicy', 'bouncy', 'forbidden',
];

const NOUNS = [
  'worm', 'orb', 'wizard', 'hat', 'potion', 'goblin', 'toad', 'spell',
  'scroll', 'mushroom', 'gremlin', 'cauldron', 'wand', 'noodle', 'familiar', 'rune',
  'dragon', 'pebble', 'moth', 'slime', 'grimoire', 'crystal', 'newt', 'bean',
];

const pick = <T>(items: readonly T[], random: () => number): T =>
  items[Math.floor(random() * items.length)];

/**
 * Builds a random alias like `sneaky-worm-42`.
 */
const summonWizardWords = (random: () => number = Math.random): string => {
  const number = Math.floor(random() * 100);
  return `${pick(ADJECTIVES, random)}-${pick(NOUNS, random)}-${number}`;
};

export { ADJECTIVES, NOUNS, summonWizardWords };
