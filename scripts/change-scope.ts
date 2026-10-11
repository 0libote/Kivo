export function changeScope(files: string[]) {
  const meaningful = files.filter(
    (file) =>
      !file.startsWith("docs/") &&
      !["README.md", "AGENTS.md", ".editorconfig", ".gitattributes"].includes(file),
  );
  const website = meaningful.some(
    (file) =>
      file.startsWith("website/") ||
      !["src/", "src-tauri/", "tests/"].some((prefix) => file.startsWith(prefix)),
  );
  const app = meaningful.filter((file) => !file.startsWith("website/"));
  const native = app.some(
    (file) =>
      file.startsWith("src-tauri/") ||
      !["src/", "tests/"].some((prefix) => file.startsWith(prefix)),
  );
  const frontend = app.length > 0;
  return { frontend, native, desktop: frontend || native, website };
}
