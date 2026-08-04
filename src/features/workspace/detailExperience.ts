export type DetailExperience = "classic" | "dashboard";

export const DETAIL_EXPERIENCE_STORAGE_KEY = "piwork.detailExperience";

export const readDetailExperience = (): DetailExperience =>
  typeof localStorage !== "undefined" &&
  localStorage.getItem(DETAIL_EXPERIENCE_STORAGE_KEY) === "classic"
    ? "classic"
    : "dashboard";

export const persistDetailExperience = (
  value: DetailExperience,
): DetailExperience => {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(DETAIL_EXPERIENCE_STORAGE_KEY, value);
  }
  return value;
};
