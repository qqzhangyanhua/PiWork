import {
  createContext,
  type PropsWithChildren,
  useContext,
  useState,
} from "react";
import { useStore } from "zustand";

import { tauriClient, type PiWorkClient } from "../../app/tauriClient";
import { createWorkStore, type WorkState, type WorkStore } from "./workStore";

type WorkStoreContextValue = {
  store: WorkStore;
  client: PiWorkClient;
};

const WorkStoreContext = createContext<WorkStoreContextValue | null>(null);

export type WorkStoreProviderProps = PropsWithChildren<{
  client?: PiWorkClient;
}>;

export function WorkStoreProvider({
  client = tauriClient,
  children,
}: WorkStoreProviderProps) {
  const [value] = useState<WorkStoreContextValue>(() => ({
    client,
    store: createWorkStore(client),
  }));

  return (
    <WorkStoreContext.Provider value={value}>
      {children}
    </WorkStoreContext.Provider>
  );
}

export const useWorkStoreContext = () => {
  const value = useContext(WorkStoreContext);
  if (!value) {
    throw new Error("useWorkStore must be used within WorkStoreProvider");
  }
  return value;
};

export const useOptionalWorkStoreContext = () => useContext(WorkStoreContext);

export const useWorkStore = <T,>(selector: (state: WorkState) => T): T => {
  const { store } = useWorkStoreContext();
  return useStore(store, selector);
};
