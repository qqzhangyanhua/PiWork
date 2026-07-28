import { WorkSurface } from "../features/workspace/WorkSurface";
import type { PiWorkClient } from "./tauriClient";

export type AppProps = {
  client?: PiWorkClient;
};

export function App({ client }: AppProps) {
  return <WorkSurface client={client} />;
}
