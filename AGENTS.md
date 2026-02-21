# AGENTS.md - Developer Guide for web-spider

## Project Overview

This is a **Vue 3 + TypeScript + Tauri** desktop application for video scraping and downloading. The frontend uses Vue 3 Composition API with `<script setup>`, and the backend is Rust (in `src-tauri/`).

---

## Build & Development Commands

### Frontend (Node.js)

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production (type check + bundle)
npm run build

# Preview production build
npm run preview

# Lint / type check only (runs vue-tsc)
npm run lint

# Clean build artifacts
npm run clean

# Clean everything (node_modules, Cargo, generated files)
npm run clean:all
```

### Desktop (Tauri)

```bash
# Start Tauri dev mode
npm run dev:desktop

# Build desktop app
npm run build:desktop

# Build debug version
npm run build:desktop:debug

# Mobile builds (iOS/Android)
npm run dev:mobile
npm run dev:ios
npm run dev:android
npm run build:ios
npm run build:android
npm run build:ios:release
npm run build:android:release
```

### Running Single Test

**Note:** This project currently has **no test framework configured**. There are no test files (`*.test.ts` or `*.spec.ts`). If adding tests, use:

```bash
# Vitest (recommended for Vue)
npm install -D vitest
npx vitest run src/services/api.test.ts

# Or with a specific file pattern
npx vitest run --filter "api"
```

---

## Code Style Guidelines

### TypeScript Configuration

The project uses strict TypeScript with these key settings (from `tsconfig.json`):

- **Strict mode**: enabled
- **No unused locals/parameters**: enforced
- **No fallthrough in switch**: enforced

### Import Conventions

```typescript
// Relative imports for local modules
import { ref, computed } from 'vue'
import ScraperView from './views/ScraperView.vue'
import type { VideoItem } from '../types'

// Tauri imports
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

// Absolute paths NOT used - always use relative imports
```

### Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Components | PascalCase | `VideoPlayer.vue`, `AddTaskDialog.vue` |
| Views | PascalCase + View suffix | `ScraperView.vue`, `SettingsView.vue` |
| Types/Interfaces | PascalCase | `VideoItem`, `AppConfig`, `YtdlpTask` |
| Enums | PascalCase | `VideoStatus`, `YtdlpTaskStatus` |
| Variables/Functions | camelCase | `currentTab`, `getVideos()` |
| Constants | camelCase or UPPER_SNAKE | `defaultQuality`, `MAX_RETRIES` |
| Files (TS) | kebab-case | `api.ts`, `video-service.ts` |

### Vue Component Structure

Use Vue 3 Composition API with `<script setup>`:

```vue
<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { VideoItem } from '../types'

const props = defineProps<{
  video: VideoItem
}>()

const emit = defineEmits<{
  (e: 'delete', id: string): void
}>()

const isLoading = ref(false)
</script>

<template>
  <!-- Template content -->
</template>

<style scoped>
/* Scoped styles */
</style>
```

### TypeScript Best Practices

1. **Always type function parameters and return values:**

   ```typescript
   // Good
   async function getVideos(page = 1, pageSize = 20): Promise<PaginatedVideos> {
     return await invoke<PaginatedVideos>('get_videos_paginated', { page, pageSize })
   }

   // Avoid
   async function getVideos(page = 1, pageSize = 20) {
     return await invoke('get_videos_paginated', { page, pageSize })
   }
   ```

2. **Use interfaces for object shapes:**

   ```typescript
   export interface VideoItem {
     id: string
     name: string
     m3u8_url: string
     status: VideoStatus
   }
   ```

3. **Use enums for fixed values:**

   ```typescript
   export enum VideoStatus {
     Pending = 'Pending',
     Downloading = 'Downloading',
     Downloaded = 'Downloaded',
   }
   ```

### Error Handling

```typescript
// Use try-catch with proper error logging
async function fetchData() {
  try {
    const result = await invoke<VideoItem>('get_video', { id })
    return result
  } catch (error) {
    console.error('Failed to fetch video:', error)
    throw error // Re-throw or handle appropriately
  }
}

// For user-facing errors, show feedback
try {
  await deleteVideo(id)
  // Show success message
} catch (error) {
  // Show error notification to user
}
```

### CSS/Style Guidelines

- Use **scoped styles** in Vue components (`<style scoped>`)
- Use **CSS custom properties** for theming when applicable
- Follow the existing color scheme:
  - Primary: `#667eea` to `#764ba2` (gradient)
  - Text: `#1a1a2e`, `#64748b`
  - Background: `#f5f7fa`
- Use **BEM-like naming** for complex components: `.nav-bar`, `.nav-tab`, `.nav-tab--active`

### API Integration (Tauri)

All Tauri backend calls go through `src/services/api.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core'
import type { VideoItem } from '../types'

export async function getVideos(page = 1, pageSize = 20): Promise<VideoItem[]> {
  return await invoke<VideoItem[]>('get_videos', { page, pageSize })
}
```

### File Organization

```
src/
├── components/     # Reusable Vue components
├── views/          # Page-level components (with View suffix)
├── services/       # API and business logic
├── types.ts        # TypeScript type definitions
├── main.ts         # Entry point
└── App.vue         # Root component
```

---

## Additional Notes

- **No ESLint/Prettier configured** - only `vue-tsc` for type checking
- **No test framework** - consider adding Vitest for future testing
- The codebase uses **Chinese comments** in `types.ts` - follow existing convention when editing that file
- When adding new Tauri commands, update both the Rust backend (`src-tauri/`) and the TypeScript API in `src/services/api.ts`
