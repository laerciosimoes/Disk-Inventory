export interface DiskInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  kind: string;
  isRemovable: boolean;
  isReadOnly: boolean;
  totalBytes: number;
  availableBytes: number;
  usedBytes: number;
  usedPercent: number;
}

export interface FsEntry {
  path: string;
  entryType: "file" | "directory" | "symlink";
  sizeBytes: number;
}

export interface TreeEntry extends FsEntry {
  name: string;
  isDir: boolean;
  isSymlink: boolean;
}

export interface AppInfo {
  name: string;
  path: string;
  isDefault: boolean;
}

export type ScanMessage =
  | { type: "start"; data: { totalBytes: number; generation: number } }
  | {
      type: "progress";
      data: { scannedFiles: number; scannedBytes: number; generation: number };
    }
  | { type: "complete"; data: { generation: number } };
