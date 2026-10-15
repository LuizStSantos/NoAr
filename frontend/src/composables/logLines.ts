/**
 * Reads the channel log as the server writes it: one line per entry,
 * `[time] [LEVEL] message`, where the message marks numbers, paths and
 * commands with `<span class="log-…">`. The markup is turned into plain parts
 * here, so the page renders text and never the file's HTML.
 */

export type LogLevel = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'FATAL'

export interface LogPart {
    text: string
    /** `log-number`, `log-addr`, ... for marked parts. */
    kind: string | null
}

export interface LogLine {
    id: number
    /** Full timestamp as written, `YYYY-MM-DD HH:MM:SS.ffffff`. */
    stamp: string
    /** `HH:MM:SS` */
    time: string
    level: LogLevel | null
    parts: LogPart[]
    /** The message as plain text, for search and copying. */
    plain: string
}

/** One row of the table: a line, or a run of near-identical lines folded under it. */
export interface LogRow {
    line: LogLine
    folded?: { lines: LogLine[]; first: string; last: string }
}

export const LEVEL_RANK: Record<LogLevel, number> = { DEBUG: 0, INFO: 1, WARN: 2, ERROR: 3, FATAL: 4 }

const LINE = /^<span class="log-gray">\[([^\]]+)\]<\/span> <span class="level-[\w-]+">\[\s*(\w+)\]<\/span> ?(.*)$/
const PART = /<span class="(log-[\w-]+)">([\s\S]*?)<\/span>/g

/** Runs at least this long fold into one row. */
const FOLD_FROM = 3

function decodeEntities(text: string): string {
    return text
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&#39;/g, "'")
        .replace(/&amp;/g, '&')
}

function plainText(markup: string): string {
    return decodeEntities(markup.replace(/<[^>]*>/g, ''))
}

function levelOf(value: string): LogLevel | null {
    const level = value.toUpperCase()

    if (level === 'WARNING') {
        return 'WARN'
    }

    return level in LEVEL_RANK ? (level as LogLevel) : null
}

function parts(message: string): LogPart[] {
    const result: LogPart[] = []
    let last = 0

    for (const match of message.matchAll(PART)) {
        const before = message.slice(last, match.index)

        if (before) {
            result.push({ text: plainText(before), kind: null })
        }

        result.push({ text: plainText(match[2] ?? ''), kind: match[1] ?? null })
        last = (match.index ?? 0) + match[0].length
    }

    const rest = message.slice(last)

    if (rest) {
        result.push({ text: plainText(rest), kind: null })
    }

    return result.filter((part) => part.text)
}

/** Parse log text; ids continue from `firstId`. Lines that do not start an
 * entry (tables, multi-line messages) are added to the entry above. */
export function parseLog(text: string, firstId = 0): LogLine[] {
    const lines: LogLine[] = []
    let id = firstId

    for (const raw of text.split(/\r?\n/)) {
        if (!raw.trim()) {
            continue
        }

        const match = raw.match(LINE)

        if (match) {
            const stamp = match[1] ?? ''
            const lineParts = parts(match[3] ?? '')

            lines.push({
                id: id++,
                stamp,
                time: stamp.slice(11, 19),
                level: levelOf(match[2] ?? ''),
                parts: lineParts,
                plain: lineParts.map((part) => part.text).join(''),
            })
            continue
        }

        const previous = lines[lines.length - 1]
        const text = plainText(raw)

        if (previous) {
            previous.parts.push({ text: `\n${text}`, kind: null })
            previous.plain += `\n${text}`
        } else {
            lines.push({ id: id++, stamp: '', time: '', level: null, parts: [{ text, kind: null }], plain: text })
        }
    }

    return lines
}

/** Lines that differ only in their numbers count as the same. */
function shape(line: LogLine): string {
    return `${line.level}|${line.plain.replace(/\d+/g, '#')}`
}

/** Fold runs of near-identical lines under their first one, except the runs
 * whose first line id is in `open`. */
export function foldRepeats(lines: LogLine[], open: Set<number>): LogRow[] {
    const rows: LogRow[] = []
    let index = 0

    while (index < lines.length) {
        const first = lines[index] as LogLine
        const key = shape(first)
        let end = index + 1

        while (end < lines.length && shape(lines[end] as LogLine) === key) {
            end++
        }

        const run = lines.slice(index, end)

        if (run.length >= FOLD_FROM && !open.has(first.id)) {
            const rest = run.slice(1)

            rows.push({ line: first })
            rows.push({
                line: first,
                folded: { lines: rest, first: rest[0]?.time ?? '', last: rest[rest.length - 1]?.time ?? '' },
            })
        } else {
            rows.push(...run.map((line) => ({ line })))
        }

        index = end
    }

    return rows
}
