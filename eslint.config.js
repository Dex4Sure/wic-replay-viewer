import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import pluginVue from 'eslint-plugin-vue';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const typedFiles = ['frontend/**/*.{ts,vue}', 'vite.config.ts'];
const parserOptions = {
  projectService: true,
  tsconfigRootDir: import.meta.dirname,
};

export default defineConfig(
  {
    ignores: [
      'local/',
      'research/',
      '.flatpak-build/',
      '.flatpak-builder/',
      '.flatpak-repo/',
      'artifacts/',
      'coverage/',
      'dist/',
      'node_modules/',
      'parser/rust_parser/pkg/',
      'public/',
      'src-tauri/gen/',
      'src-tauri/icons/',
      'target/',
    ],
  },
  {
    files: ['eslint.config.js', 'scripts/**/*.{js,mjs}'],
    ...js.configs.recommended,
    languageOptions: {
      ecmaVersion: 'latest',
      globals: globals.node,
      sourceType: 'module',
    },
    rules: {
      curly: ['error', 'all'],
      eqeqeq: ['error', 'always'],
    },
  },
  ...tseslint.configs.recommendedTypeChecked.map((config) => ({
    ...config,
    files: typedFiles,
  })),
  ...pluginVue.configs['flat/essential'],
  {
    files: typedFiles,
    languageOptions: {
      globals: globals.browser,
      parserOptions,
    },
    rules: {
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          fixStyle: 'inline-type-imports',
          prefer: 'type-imports',
        },
      ],
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-unnecessary-type-assertion': 'off',
      curly: ['error', 'all'],
      eqeqeq: ['error', 'always'],
      'no-console': 'error',
    },
  },
  {
    files: ['frontend/**/*.vue'],
    languageOptions: {
      parserOptions: {
        ...parserOptions,
        extraFileExtensions: ['.vue'],
        parser: tseslint.parser,
      },
    },
    rules: {
      'vue/component-name-in-template-casing': [
        'error',
        'PascalCase',
        {
          registeredComponentsOnly: true,
        },
      ],
    },
  },
  {
    files: ['frontend/**/*.test.ts'],
    rules: {
      // Vue Test Utils exposes component instances through framework-generated
      // types that type-aware ESLint treats as unresolved. Assertions still
      // compile under vue-tsc; relax only the unsafe-family rules in tests.
      '@typescript-eslint/consistent-type-imports': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/require-await': 'off',
    },
  },
  {
    files: ['frontend/main.ts'],
    rules: {
      '@typescript-eslint/no-unsafe-argument': 'off',
    },
  },
  {
    files: ['vite.config.ts'],
    languageOptions: {
      globals: globals.node,
    },
  },
);
