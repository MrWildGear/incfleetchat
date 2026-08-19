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
  recordSessionTracking: async (input) => {
    const board = get().board;
    const postedAtDate = new Date(input.posted_at);
    const expiresAtDate = new Date(input.expires_at);

    // 1. Duplication Check (High Priority): Prevent creating duplicate records based on date/time.
    const isDuplicate = board.some((site) => {
      if (!site || !site.posted_at || !site.expires_at) return false;
      try {
        const existingPostedAt = new Date(site.posted_at);
        const existingExpiresAt = new Date(site.expires_at);

        // Use date-only comparison (ignoring time component differences)
        return existingPostedAt.getFullYear() === postedAtDate.getFullYear() &&
               existingPostedAt.getMonth() === postedAtDate.getMonth() &&
               existingPostedAt.getDay() === postedAtDate.getDay() &&
               existingExpiresAt.getFullYear() === expiresAtDate.getFullYear() &&
               existingExpiresAt.getMonth() === expiresAtDate.getMonth() &&
               existingExpiresAt.getDay() === expiresAtDate.getDay();
      } catch (e) {
        console.error("Error comparing dates during duplicate check:", e);
        return false;
      }
    });

    if (isDuplicate) {
      throw new Error("Duplicate site record detected: Site already tracked with this posted and expires date.");
    }

    // Proceed with the original logic if no duplication is found.
    const event = await invoke<SessionTrackingEvent>("record_session_tracking_event", { input });
    set({ board: { ...board, ...event.newSite } }); // Assuming the event payload contains new site data for merging/updating
    return event;
  },
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
