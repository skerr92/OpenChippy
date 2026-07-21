export function isEditableTarget(target: EventTarget | null): boolean {
  if (!target || typeof (target as Element).closest !== "function") return false;
  return Boolean((target as Element).closest("input, textarea, select, [contenteditable]:not([contenteditable='false'])"));
}

export function isEditingText(event: KeyboardEvent): boolean {
  if (isEditableTarget(event.target) || isEditableTarget(document.activeElement)) return true;
  return event.composedPath().some(isEditableTarget);
}
