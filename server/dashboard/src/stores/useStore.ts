import { create } from 'zustand';
import type { Bot, ClusterStatus, User } from '@/types';

interface AppState {
  // Auth
  user: User | null;
  setUser: (user: User | null) => void;

  // Cluster status
  status: ClusterStatus | null;
  setStatus: (status: ClusterStatus | null) => void;

  // Bots
  bots: Bot[];
  setBots: (bots: Bot[]) => void;
  updateBot: (bot: Bot) => void;

  // WebSocket
  wsConnected: boolean;
  setWsConnected: (connected: boolean) => void;

  // UI
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
}

export const useStore = create<AppState>((set) => ({
  // Auth
  user: null,
  setUser: (user) => set({ user }),

  // Cluster status
  status: null,
  setStatus: (status) => set({ status }),

  // Bots
  bots: [],
  setBots: (bots) => set({ bots }),
  updateBot: (bot) => set((state) => ({
    bots: state.bots.map((b) => (b.id === bot.id ? bot : b)),
  })),

  // WebSocket
  wsConnected: false,
  setWsConnected: (connected) => set({ wsConnected: connected }),

  // UI
  sidebarOpen: true,
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
}));
