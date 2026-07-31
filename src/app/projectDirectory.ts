import { open } from "@tauri-apps/plugin-dialog";

export type PickProjectDirectory = () => Promise<string | null>;

export const pickProjectDirectory: PickProjectDirectory = async () => {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  return typeof selected === "string" ? selected : null;
};
