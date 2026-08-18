import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Board,
  OverlaySettings,
  RecordSessionTrackingInput,
  SessionTrackingEvent,
} from "./lib/types";
import { emptyBoard } from "./lib/types";

type Store = {
  board: Board;
  settings: OverlaySettings | null;
  characters: string[];
  settingsOpen: boolean;
  nowMs: number;
  setBoard: (board: Board) => void;
  setSettingsOpen: (open: boolean) => void;
  tick: () => void;
  hydrate: () => Promise<void>;
  markRan: (siteId: string) => Promise<void>;
  recordSessionTracking: (
    input: RecordSessionTrackingInput,
  ) => Promise<SessionTrackingEvent>;
  clearSite: (siteId: string) => Promise<void>;
  clearReady: () => Promise<void>;
  loadCharacters: () => Promise<void>;
  saveSettings: (patch: {
    character?: string | null;
    chatlogs_dir?: string | null;
    always_on_top?: boolean;
    tracking_pip_enabled?: boolean;
  }) => Promise<void>;
  setAlwaysOnTop: (on: boolean) => Promise<void>;
};

export const useAppStore = create<Store>((set, get) => ({
  board: emptyBoard,
  settings: null,
  characters: [],
  settingsOpen: false,
  nowMs: Date.now(),
  setBoard: (board) => set({ board }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  tick: () => set({ nowMs: Date.now() }),
  hydrate: async () => {
    const [board, settings] = await Promise.all([
      invoke<Board>("get_board"),
      invoke<OverlaySettings>("get_overlay_settings"),
    ]);
    set({
      board,
      settings,
      settingsOpen: !settings.character,
    });
    await listen<Board>("board-updated", (e) => {
      set({ board: e.payload });
    });
  },
  markRan: async (siteId) => {
    const board = await invoke<Board>("mark_ran", { siteId });
    set({ board });
  },
  recordSessionTracking: async (input) =>
    invoke<SessionTrackingEvent>("record_session_tracking_event", { input }),
  clearSite: async (siteId) => {
    const board = await invoke<Board>("clear_site", { siteId });
    set({ board });
  },
  clearReady: async () => {
    const board = await invoke<Board>("clear_ready");
    set({ board });
  },
  loadCharacters: async () => {
    const characters = await invoke<string[]>("list_characters");
    set({ characters });
  },
  saveSettings: async (patch) => {
    const payload: Record<string, unknown> = {};
    if ("character" in patch) payload.character = patch.character;
    if ("chatlogs_dir" in patch) payload.chatlogs_dir = patch.chatlogs_dir;
    if ("always_on_top" in patch) payload.always_on_top = patch.always_on_top;
    if ("tracking_pip_enabled" in patch) {
      payload.tracking_pip_enabled = patch.tracking_pip_enabled;
    }
    const settings = await invoke<OverlaySettings>("set_overlay_settings", {
      patch: payload,
    });
    set({ settings, settingsOpen: false });
    const board = await invoke<Board>("get_board");
    set({ board });
  },
  setAlwaysOnTop: async (on) => {
    await invoke("set_always_on_top", { on });
    const settings = get().settings;
    if (settings) set({ settings: { ...settings, always_on_top: on } });
  },
}));
