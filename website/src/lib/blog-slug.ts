export function blogSlugFromId(id: string): string {
  return id.replace(/\.md$/, "");
}
