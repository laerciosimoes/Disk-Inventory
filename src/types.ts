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
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  sizeBytes: number;
}
