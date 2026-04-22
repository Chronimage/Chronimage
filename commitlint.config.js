/** @type {import('@commitlint/types').UserConfig} */
export default {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'type-enum': [
      2,
      'always',
      ['feat', 'fix', 'perf', 'refactor', 'test', 'docs', 'style', 'ci', 'build', 'chore', 'revert'],
    ],
    'scope-enum': [
      2,
      'always',
      [
        // frontend
        'ui',
        'chrome',
        'primitives',
        'screens',
        'state',
        'tauri-cli',
        'styles',
        // backend
        'catalog',
        'import',
        'ai',
        'raw',
        'dedupe',
        'faces',
        'sidecar',
        'cullbin',
        'develop',
        'export',
        'entitlements',
        // infra
        'ci',
        'release',
        'hooks',
        'docs',
        'deps',
        'scripts',
        'repo',
      ],
    ],
    'scope-empty': [2, 'never'],
    // Allow sentence-case too — dependabot hardcodes "Bump X from Y to Z"
    // with a capital B, and other renovate-style bots do similar. Keeping
    // the rule strictly lower-case blocks every such PR on commitlint.
    'subject-case': [2, 'always', ['lower-case', 'sentence-case']],
    'subject-max-length': [2, 'always', 100],
    'body-max-line-length': [1, 'always', 120],
  },
};
