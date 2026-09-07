import { shell } from "./runtime";

export default async function Command() {
  await shell(["center"]);
}
