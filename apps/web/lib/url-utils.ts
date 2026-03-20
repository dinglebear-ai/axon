/** Check if a string is an HTTP(S) URL */
export function isHttpUrl(url: string): boolean {
  return /^https?:\/\//i.test(url)
}
