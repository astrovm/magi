module.exports = {
    root: true,
    parser: '@typescript-eslint/parser',
    parserOptions: {
        tsconfigRootDir: __dirname,
        project: ['./tsconfig.json']
    },
    plugins: [
        '@typescript-eslint'
    ],
    extends: [
        'airbnb',
        'airbnb-typescript',
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended'
    ],
};
