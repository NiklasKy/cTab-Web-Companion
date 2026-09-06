export function appendText(parent: HTMLElement, tagName: "div" | "span" | "strong", value: string, className?: string): HTMLElement {
  const node = document.createElement(tagName);
  if (className !== undefined) {
    node.className = className;
  }
  node.textContent = value;
  parent.append(node);
  return node;
}

export function replaceText(parent: HTMLElement, value: string): void {
  parent.replaceChildren(document.createTextNode(value));
}
