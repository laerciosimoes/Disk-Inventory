/**
 * Treemap tiles are colored by file type/category, so the same kind of
 * content (photos, videos, code, archives, ...) always reads the same
 * color no matter where in the tree it lives.
 *
 * Directories are the one exception: a folder isn't "a type of data" the
 * way a file is, so instead of one flat color for every folder (which made
 * sibling folders indistinguishable from each other), each folder gets its
 * own muted, hash-derived hue — distinct from its neighbors, but visibly
 * less saturated than the vivid file-type colors so folders still read as
 * containers rather than data.
 */

export type FileCategory =
  | "directory"
  | "image"
  | "video"
  | "audio"
  | "document"
  | "spreadsheet"
  | "archive"
  | "code"
  | "app"
  | "font"
  | "other";

/** macOS/Linux "directory that behaves like a single file" bundles. */
const BUNDLE_EXTENSIONS = new Set([
  "app",
  "framework",
  "bundle",
  "plugin",
  "prefpane",
]);

const EXTENSION_CATEGORIES: Record<string, FileCategory> = {
  // Images
  jpg: "image",
  jpeg: "image",
  png: "image",
  gif: "image",
  webp: "image",
  bmp: "image",
  tiff: "image",
  tif: "image",
  svg: "image",
  heic: "image",
  heif: "image",
  ico: "image",
  psd: "image",
  ai: "image",
  raw: "image",

  // Video
  mp4: "video",
  mov: "video",
  avi: "video",
  mkv: "video",
  wmv: "video",
  flv: "video",
  webm: "video",
  m4v: "video",
  mpg: "video",
  mpeg: "video",

  // Audio
  mp3: "audio",
  wav: "audio",
  aac: "audio",
  flac: "audio",
  m4a: "audio",
  ogg: "audio",
  wma: "audio",
  aiff: "audio",

  // Documents
  pdf: "document",
  doc: "document",
  docx: "document",
  txt: "document",
  md: "document",
  rtf: "document",
  pages: "document",
  odt: "document",
  epub: "document",

  // Spreadsheets / presentations
  xls: "spreadsheet",
  xlsx: "spreadsheet",
  csv: "spreadsheet",
  ppt: "spreadsheet",
  pptx: "spreadsheet",
  key: "spreadsheet",
  numbers: "spreadsheet",

  // Archives / disk images
  zip: "archive",
  tar: "archive",
  gz: "archive",
  bz2: "archive",
  xz: "archive",
  rar: "archive",
  "7z": "archive",
  dmg: "archive",
  pkg: "archive",
  iso: "archive",

  // Code / config
  js: "code",
  ts: "code",
  jsx: "code",
  tsx: "code",
  vue: "code",
  py: "code",
  rs: "code",
  go: "code",
  java: "code",
  kt: "code",
  c: "code",
  h: "code",
  cpp: "code",
  hpp: "code",
  swift: "code",
  rb: "code",
  php: "code",
  sh: "code",
  sql: "code",
  html: "code",
  css: "code",
  scss: "code",
  json: "code",
  yaml: "code",
  yml: "code",
  toml: "code",
  xml: "code",

  // Fonts
  ttf: "font",
  otf: "font",
  woff: "font",
  woff2: "font",

  // Standalone executables (bundle-style apps are handled separately below)
  exe: "app",
};

interface CategoryStyle {
  hue: number;
  saturation: number;
}

const CATEGORY_STYLES: Record<Exclude<FileCategory, "directory">, CategoryStyle> = {
  other: { hue: 30, saturation: 10 }, // neutral warm gray — data of an unrecognized kind
  image: { hue: 142, saturation: 55 },
  video: { hue: 350, saturation: 60 },
  audio: { hue: 271, saturation: 48 },
  document: { hue: 205, saturation: 55 },
  spreadsheet: { hue: 45, saturation: 62 },
  archive: { hue: 25, saturation: 58 },
  code: { hue: 185, saturation: 50 },
  app: { hue: 320, saturation: 48 },
  font: { hue: 255, saturation: 42 },
};

// Curated hues rather than a raw hash-to-360 spread, so neighboring
// folders don't land on muddy/adjacent colors.
const DIRECTORY_HUES = [4, 24, 45, 100, 160, 190, 213, 250, 280, 320, 340, 60];
const DIRECTORY_SATURATION = 22; // muted relative to file-type colors, so folders read as containers

function hashString(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i++) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function directoryStyle(path: string): CategoryStyle {
  const hue = DIRECTORY_HUES[hashString(path) % DIRECTORY_HUES.length];
  return { hue, saturation: DIRECTORY_SATURATION };
}

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  // No extension, or a dotfile like ".DS_Store" (leading dot, nothing after
  // it is a "real" extension in the usual sense).
  if (dot <= 0) return "";
  return name.slice(dot + 1).toLowerCase();
}

export function categoryForEntry(name: string, isDir: boolean): FileCategory {
  const ext = extensionOf(name);

  if (isDir) {
    return BUNDLE_EXTENSIONS.has(ext) ? "app" : "directory";
  }

  return EXTENSION_CATEGORIES[ext] ?? "other";
}

interface ColorableEntry {
  path: string;
  name: string;
  isDir: boolean;
}

function styleForEntry(entry: ColorableEntry): CategoryStyle {
  const category = categoryForEntry(entry.name, entry.isDir);

  if (category === "directory") {
    return directoryStyle(entry.path);
  }

  return CATEGORY_STYLES[category];
}

/**
 * Every file's color is derived from its file-type category, so the same
 * kind of content always reads the same color no matter where in the tree
 * it lives; every folder's color is derived from its own path instead (see
 * the module doc comment above). Depth only adjusts lightness, giving the
 * nested "cushion" look without the color identity drifting per level.
 */
export function backgroundForEntry(entry: ColorableEntry, depth: number): string {
  const { hue, saturation } = styleForEntry(entry);
  const lightness = Math.min(30 + depth * 6, 62);
  const highlight = Math.min(lightness + 20, 82);
  return `radial-gradient(circle at 32% 26%, hsl(${hue}, ${saturation + 7}%, ${highlight}%) 0%, hsl(${hue}, ${saturation}%, ${lightness}%) 75%)`;
}

export function swatchForEntry(entry: ColorableEntry): string {
  const { hue, saturation } = styleForEntry(entry);
  return `hsl(${hue}, ${saturation}%, 45%)`;
}
