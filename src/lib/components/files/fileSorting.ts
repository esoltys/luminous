import type { FileEntry } from "../../types";

export type FileSortColumn = "name" | "fileType" | "size" | "status";

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/**
 * Returns a sorted copy of file entries, maintaining the invariant that
 * folder rows always remain grouped at the top of the browser table.
 *
 * - When sorting by "name", both folders and files sort by name in the specified direction.
 * - When sorting by other columns ("fileType", "size", "status"), folders lack those attributes
 *   and sort alphabetically by name, while files sort by the chosen attribute with name tiebreaking.
 */
export function sortFileEntries(
  entries: FileEntry[],
  column: FileSortColumn,
  asc: boolean,
  getStatusLabel?: (entry: FileEntry) => string
): FileEntry[] {
  const dir = asc ? 1 : -1;
  const folders = entries.filter((e) => e.isDir);
  const files = entries.filter((e) => !e.isDir);

  const sortedFolders = [...folders].sort((a, b) => {
    // When sorting by name, folders respect the sort direction.
    // When sorting by other attributes (which folders don't have), folders stay in ascending alphabetical order.
    const folderDir = column === "name" ? dir : 1;
    return collator.compare(a.name, b.name) * folderDir;
  });

  const sortedFiles = [...files].sort((a, b) => {
    let diff = 0;
    switch (column) {
      case "name":
        diff = collator.compare(a.name, b.name);
        break;
      case "fileType": {
        const typeA = a.fileType ?? "";
        const typeB = b.fileType ?? "";
        diff = collator.compare(typeA, typeB);
        break;
      }
      case "size": {
        const sizeA = a.size ?? 0;
        const sizeB = b.size ?? 0;
        diff = sizeA - sizeB;
        break;
      }
      case "status": {
        const labelA = getStatusLabel ? getStatusLabel(a) : (a.status ?? "");
        const labelB = getStatusLabel ? getStatusLabel(b) : (b.status ?? "");
        diff = collator.compare(labelA, labelB);
        break;
      }
    }

    if (diff !== 0) {
      return diff * dir;
    }
    // Consistent tiebreaker by entry name
    return collator.compare(a.name, b.name);
  });

  return [...sortedFolders, ...sortedFiles];
}
