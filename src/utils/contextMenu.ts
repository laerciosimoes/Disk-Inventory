import { Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, TreeEntry } from "../types";
import { useFsTree } from "../composables/fsTree";

/**
 * Zooms back out past everything that lived under `removedPath` (which has
 * just been trashed), so the treemap never keeps pointing at a path that no
 * longer exists in the scan index.
 */
function zoomOutOfRemovedSubtree(removedPath: string): void {
  const tree = useFsTree();

  while (
    tree.zoomRoot.value === removedPath ||
    tree.zoomRoot.value?.startsWith(removedPath + "/")
  ) {
    if (!tree.canZoomOut()) {
      tree.zoomToRoot();
      break;
    }
    tree.zoomOut();
  }
}

async function buildOpenWithSubmenu(entry: TreeEntry): Promise<Submenu> {
  let apps: AppInfo[] = [];
  try {
    apps = await invoke<AppInfo[]>("list_apps_for_path", { path: entry.path });
  } catch {
    // Fall through with an empty list — the submenu just shows nothing to pick.
  }

  const items = apps.length
    ? await Promise.all(
        apps.map((app) =>
          MenuItem.new({
            text: app.name,
            action: () => {
              void invoke("open_path_with", { path: entry.path, appPath: app.path });
            },
          })
        )
      )
    : [await MenuItem.new({ text: "No applications found", enabled: false })];

  return Submenu.new({ text: "Open With", items });
}

/** Shows the native right-click menu for a file-tree/treemap entry. */
export async function showEntryContextMenu(entry: TreeEntry, event: MouseEvent): Promise<void> {
  event.preventDefault();

  const tree = useFsTree();
  tree.select(entry.path, entry.isDir);

  const parent = tree.parentPath(entry.path);

  const items = [
    await MenuItem.new({
      text: "Open",
      action: () => {
        void invoke("open_path", { path: entry.path });
      },
    }),
    await buildOpenWithSubmenu(entry),
    await PredefinedMenuItem.new({ item: "Separator" }),
    await MenuItem.new({
      text: "Reveal in Finder",
      action: () => {
        void invoke("reveal_in_finder", { path: entry.path });
      },
    }),
    await MenuItem.new({
      text: "Refresh",
      action: () => {
        void tree.refreshChildren(entry.isDir ? entry.path : parent);
      },
    }),
    await PredefinedMenuItem.new({ item: "Separator" }),
    await MenuItem.new({
      text: "Move To Trash",
      action: async () => {
        await invoke("move_to_trash", { path: entry.path });
        zoomOutOfRemovedSubtree(entry.path);
        if (tree.selectedPath.value === entry.path) {
          tree.select(null);
        }
        await tree.refreshChildren(parent);
      },
    }),
    await PredefinedMenuItem.new({ item: "Separator" }),
    await MenuItem.new({
      text: "Zoom In",
      enabled: entry.isDir,
      action: () => {
        void tree.zoomIn(entry.path);
      },
    }),
    await MenuItem.new({
      text: "Zoom Out",
      enabled: tree.canZoomOut(),
      action: () => tree.zoomOut(),
    }),
  ];

  const menu = await Menu.new({ items });
  await menu.popup();
}
