import { open } from "@tauri-apps/plugin-dialog";

export type PickAttachments = () => Promise<string[]>;

export const ATTACHMENT_EXTENSIONS = [
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "pdf",
  "docx",
  "xls",
  "xlsx",
  "xlsm",
  "xlsb",
  "csv",
] as const;

export const pickAttachments: PickAttachments = async () => {
  const selected = await open({
    directory: false,
    multiple: true,
    filters: [
      {
        name: "Supported files",
        extensions: [...ATTACHMENT_EXTENSIONS],
      },
    ],
  });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
};
