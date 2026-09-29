import { invoke } from '@tauri-apps/api/core';

/** A screen's place on the desk, in millimetres. */
export type Desk = { x: number; y: number; w: number; h: number };

export type Screen = {
  id: string;
  name: string;
  number: number;
  primary: boolean;
  scale: number;
  px: { x: number; y: number; w: number; h: number };
  detectedMm: [number, number];
  sizeFromEdid: boolean;
  desk: Desk;
};

export type Theme = 'system' | 'light' | 'dark';
export type Material = 'acrylic' | 'mica' | 'solid';
export type Appearance = { theme: Theme; accent: string; material: Material };

export type State = {
  enabled: boolean;
  pauseInFullscreen: boolean;
  checkUpdates: boolean;
  startWithWindows: boolean;
  appearance: Appearance;
  systemAccent: string;
  mica: boolean;
  trayRunning: boolean;
  version: string;
  screens: Screen[];
  unreachable: string[];
};

export const api = {
  state: () => invoke<State>('get_state'),
  saveLayout: (desk: (Desk & { id: string })[]) =>
    invoke<{ unreachable: string[]; line_mm: number }>('save_layout', { desk }),
  autoLayout: () => invoke<Desk[]>('auto_layout'),
  setEnabled: (on: boolean) => invoke<void>('set_enabled', { on }),
  setPauseInFullscreen: (on: boolean) => invoke<void>('set_pause_in_fullscreen', { on }),
  setCheckUpdates: (on: boolean) => invoke<void>('set_check_updates', { on }),
  launchMode: () => invoke<'settings' | 'update'>('launch_mode'),
  setStartWithWindows: (on: boolean) => invoke<boolean>('set_start_with_windows', { on }),
  setAppearance: (appearance: Appearance) => invoke<void>('set_appearance', { appearance }),
  applyMaterial: (material: Material, dark: boolean) => invoke<Material>('apply_material', { material, dark }),
  setAlignmentLine: (on: boolean) => invoke<boolean>('set_alignment_line', { on }),
  open: (which: 'source' | 'issues' | 'folder') => invoke<void>('open_link', { which }),
};
