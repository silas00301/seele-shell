import { shell } from "./runtime";

// The bar says the mode is on, so the command only flips it.
export default async function Command() {
  await shell(["presentation", "toggle"]);
}
