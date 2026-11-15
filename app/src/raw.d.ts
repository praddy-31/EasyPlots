// Files imported as text by Vite (`import text from "./file.md?raw"`).
declare module "*?raw" {
  const text: string;
  export default text;
}

// Vite: a file's URL in the built app (`docs/command-map.svg?url` for the Help tab).
declare module "*?url" {
  const url: string;
  export default url;
}
