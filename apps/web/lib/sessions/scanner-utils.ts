import { readdir, stat } from 'node:fs/promises'

export async function safeReaddir(dir: string): Promise<string[]> {
  try {
    return await readdir(dir)
  } catch {
    return []
  }
}

export async function isDir(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isDirectory()
  } catch {
    return false
  }
}
