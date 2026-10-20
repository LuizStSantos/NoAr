import dayjs from 'dayjs'
import type { RowInput } from 'jspdf-autotable'

import {
    clock,
    dayLabel,
    dayText,
    displayTitle,
    duration,
    fullDuration,
    isPast,
    itemDetail,
    itemSeconds,
    itemTitle,
    kindText,
    originText,
    shortDate,
    statusText,
    type DaySummary,
    type ReportSummary,
} from '@/composables/report'

type Translate = (key: string, values?: Record<string, unknown>, plural?: number) => string

/** Everything a downloaded report needs. */
export interface ReportExport {
    t: Translate
    /** BCP 47 language of the interface, e.g. `pt-BR`. */
    locale: string
    report: Report
    channel: string
    /** The items as filtered on screen. */
    rows: ReportItem[]
    /** The programme on air before each item. */
    previous: Map<ReportItem, ReportItem>
    summary: ReportSummary
    /** Set for a period: its days, one summary each. */
    days: DaySummary[] | null
    /** Items the report is narrowed to. */
    picks: ReportPick[]
    /** The range in words, e.g. `20/10/2026` or `01/10/2026 a 31/10/2026`. */
    range: string
    /** The filters in words. */
    filters: string
    /** File name without extension. */
    file: string
}

function save(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')

    link.href = url
    link.download = name
    document.body.appendChild(link)
    link.click()
    document.body.removeChild(link)
    setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/**
 * A spreadsheet: one line per item, in UTF-8 with a byte order mark so
 * Excel reads the accents. Portuguese spreadsheets expect semicolons.
 */
export function downloadCsv(data: ReportExport) {
    const { t, report, locale } = data
    const separator = locale.startsWith('pt') ? ';' : ','
    const cell = (value: string) => `"${value.replace(/"/g, '""')}"`
    const header = [
        t('report.csv.date'),
        t('report.csv.start'),
        t('report.csv.end'),
        t('report.csv.duration'),
        t('report.csv.title'),
        t('report.csv.episode'),
        t('report.csv.file'),
        t('report.csv.kind'),
        t('report.csv.origin'),
        t('report.csv.event'),
        t('report.csv.after'),
        t('report.csv.status'),
        t('report.csv.note'),
    ]
    const lines = data.rows.map((item) => {
        const status = statusText(item, t, report.timezone, report.now)
        const previous = data.previous.get(item)
        const note = [status.note, item.status === 'failed' ? (item.detail ?? '') : ''].filter(Boolean).join(' · ')

        return [
            shortDate(dayjs(item.start).tz(report.timezone).format('YYYY-MM-DD'), locale),
            clock(item.start, report.timezone),
            clock(item.end, report.timezone),
            fullDuration(itemSeconds(item)),
            item.kind === 'gap' || item.kind === 'filler' ? itemTitle(item, t) : item.title,
            item.sub_title ?? '',
            item.kind === 'filler' ? (item.replaces ?? '') : item.kind === 'gap' ? '' : item.source,
            kindText(item, t),
            item.origin === 'schedule' ? t('report.origin.schedule') : originText(item, t),
            item.event_title ?? '',
            previous ? displayTitle(previous) : '',
            status.label,
            note,
        ]
    })
    const text = [header, ...lines].map((line) => line.map(cell).join(separator)).join('\r\n')

    save(new Blob(['﻿', text, '\r\n'], { type: 'text/csv;charset=utf-8' }), `${data.file}.csv`)
}

const INK = 26
const GRAY = 74
const LIGHT = 200

/** A printable A4 report, built in the browser. */
export async function downloadPdf(data: ReportExport) {
    const [{ jsPDF }, { autoTable }] = await Promise.all([import('jspdf'), import('jspdf-autotable')])
    const { t, report, locale } = data
    const doc = new jsPDF({ unit: 'pt', format: 'a4' })
    const margin = 40
    const width = doc.internal.pageSize.getWidth()
    const height = doc.internal.pageSize.getHeight()
    const generated = dayjs(report.now).tz(report.timezone)
    const picked = data.picks.length > 0
    const itemNames = data.picks.map((pick) => pick.label).join(', ')

    // Header
    doc.setTextColor(INK)
    doc.setFont('helvetica', 'bold')
    doc.setFontSize(16)
    doc.text(data.channel, margin, margin + 6)
    doc.setFont('helvetica', 'normal')
    doc.setFontSize(11)
    doc.text(`${picked ? t('report.pdf.titleItem') : t('report.pdf.title')} · ${data.range}`, margin, margin + 24)
    doc.setFontSize(9)
    doc.setTextColor(GRAY)
    doc.text(
        t('report.pdf.days', { start: report.day_start.slice(0, 5), timezone: report.timezone }),
        margin,
        margin + 38,
    )
    doc.setFont('helvetica', 'bold')
    doc.setTextColor(INK)
    doc.setFontSize(10)
    doc.text('NoAr', width - margin, margin + 6, { align: 'right' })
    doc.setFont('helvetica', 'normal')
    doc.setFontSize(9)
    doc.setTextColor(GRAY)
    doc.text(
        t('report.pdf.generated', {
            date: shortDate(generated.format('YYYY-MM-DD'), locale),
            time: generated.format('HH:mm'),
        }),
        width - margin,
        margin + 24,
        { align: 'right' },
    )
    doc.text(
        doc.splitTextToSize(t('report.pdf.filters', { filters: data.filters }), 220),
        width - margin,
        margin + 38,
        { align: 'right' },
    )
    doc.setDrawColor(INK)
    doc.setLineWidth(1.5)
    doc.line(margin, margin + 50, width - margin, margin + 50)

    // Summary
    const summary = data.summary
    const summaryCells = picked
        ? [
              [t('report.pdf.items'), itemNames],
              [
                  t('report.summary.total'),
                  `${summary.aired.count + summary.planned.count}× · ${duration(summary.aired.seconds + summary.planned.seconds)}`,
              ],
              [t('report.summary.aired'), `${summary.aired.count}× · ${duration(summary.aired.seconds)}`],
              [t('report.summary.planned'), `${summary.planned.count}× · ${duration(summary.planned.seconds)}`],
          ]
        : [
              [
                  t('report.summary.aired'),
                  `${duration(summary.aired.seconds)} · ${t('report.summary.items', { count: summary.aired.count }, summary.aired.count)}`,
              ],
              [
                  t('report.summary.planned'),
                  `${duration(summary.planned.seconds)} · ${t('report.summary.items', { count: summary.planned.count }, summary.planned.count)}`,
              ],
              [
                  t('report.summary.commercials'),
                  `${summary.commercials.count} · ${duration(summary.commercials.seconds)}`,
              ],
              [t('report.summary.lives'), `${summary.lives.count} · ${duration(summary.lives.seconds)}`],
              [
                  t('report.summary.occurrences'),
                  t('report.summary.occurrencesText', { cut: summary.cut, failed: summary.failed }),
              ],
          ]

    autoTable(doc, {
        startY: margin + 62,
        margin: { left: margin, right: margin },
        head: [summaryCells.map((cell) => cell[0] ?? '')],
        body: [summaryCells.map((cell) => cell[1] ?? '')],
        theme: 'grid',
        styles: { fontSize: 9, textColor: INK, lineColor: LIGHT, lineWidth: 0.75, cellPadding: 5 },
        headStyles: { fillColor: [255, 255, 255], textColor: GRAY, fontStyle: 'normal' },
        bodyStyles: { fontStyle: 'bold' },
    })

    // Day by day, for a period
    if (data.days) {
        const days = data.days

        autoTable(doc, {
            margin: { left: margin, right: margin, bottom: 50 },
            head: [[t('report.pdf.day'), t('report.pdf.count'), t('report.pdf.time'), t('report.pdf.status')]],
            body: [
                ...days.map((day) => [
                    dayLabel(day.day.date, locale, true),
                    String(day.count),
                    duration(day.seconds),
                    dayText(day, t),
                ]),
                [
                    t('report.pdf.total'),
                    String(days.reduce((sum, day) => sum + day.count, 0)),
                    duration(days.reduce((sum, day) => sum + day.seconds, 0)),
                    '',
                ],
            ],
            ...tableStyle(),
            columnStyles: { 1: { halign: 'right' }, 2: { halign: 'right' } },
            didParseCell: (cell) => {
                if (cell.section === 'body' && cell.row.index === days.length) {
                    cell.cell.styles.fontStyle = 'bold'
                }
            },
        })
    }

    // Item by item
    const period = data.days !== null
    const head = [
        ...(period ? [t('report.pdf.day')] : []),
        t('report.pdf.start'),
        t('report.pdf.end'),
        t('report.pdf.duration'),
        picked ? t('report.pdf.after') : t('report.pdf.titleColumn'),
        picked ? t('report.pdf.origin') : t('report.pdf.kind'),
        t('report.pdf.status'),
    ]
    const firstPlanned = data.rows.findIndex((item) => !isPast(item))
    const body: RowInput[] = []
    const plannedRows = new Set<number>()

    data.rows.forEach((item, index) => {
        if (index === firstPlanned && index > 0) {
            body.push([
                {
                    content: t('report.pdf.fromNow'),
                    colSpan: head.length,
                    styles: { fontStyle: 'bold', lineColor: INK, lineWidth: { top: 1.25, bottom: 0.75 } },
                },
            ])
        }

        const status = statusText(item, t, report.timezone, report.now)
        const previous = data.previous.get(item)

        if (!isPast(item)) {
            plannedRows.add(body.length)
        }

        body.push([
            ...(period ? [shortDate(item.day, locale)] : []),
            clock(item.start, report.timezone),
            clock(item.end, report.timezone),
            duration(itemSeconds(item)),
            picked ? (previous ? displayTitle(previous) : '') : rowTitle(item, t),
            picked ? originText(item, t) : kindText(item, t),
            [status.label, status.note].filter(Boolean).join(' · '),
        ])
    })

    autoTable(doc, {
        margin: { left: margin, right: margin, bottom: 50 },
        head: [head],
        body,
        ...tableStyle(),
        columnStyles: period ? { 3: { halign: 'right' } } : { 2: { halign: 'right' } },
        didParseCell: (cell) => {
            if (cell.section !== 'body') {
                return
            }

            if (plannedRows.has(cell.row.index)) {
                cell.cell.styles.textColor = GRAY
            }
        },
    })

    // Closing note
    const endY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY ?? margin
    const notes = [
        t('report.pdf.note', {
            date: shortDate(generated.format('YYYY-MM-DD'), locale),
            time: generated.format('HH:mm'),
        }),
    ]

    if (data.rows.some((item) => item.state === 'unrecorded') && report.recorded_since) {
        const since = dayjs(report.recorded_since).tz(report.timezone)

        notes.push(
            t('report.pdf.unrecorded', {
                date: shortDate(since.format('YYYY-MM-DD'), locale),
                time: since.format('HH:mm'),
            }),
        )
    }

    doc.setFontSize(8.5)
    doc.setTextColor(GRAY)

    let noteY = endY + 16

    for (const note of notes) {
        const lines = doc.splitTextToSize(note, width - margin * 2)

        if (noteY + lines.length * 11 > height - 50) {
            doc.addPage()
            noteY = margin
        }

        doc.text(lines, margin, noteY)
        noteY += lines.length * 11 + 4
    }

    // Page numbers
    const pages = doc.getNumberOfPages()

    for (let page = 1; page <= pages; page++) {
        doc.setPage(page)
        doc.setFontSize(8.5)
        doc.setTextColor(GRAY)
        doc.setDrawColor(LIGHT)
        doc.setLineWidth(0.75)
        doc.line(margin, height - 34, width - margin, height - 34)
        doc.text(`NoAr · ${data.channel} · ${data.range}`, margin, height - 22)
        doc.text(t('report.pdf.page', { page, pages }), width - margin, height - 22, { align: 'right' })
    }

    save(doc.output('blob'), `${data.file}.pdf`)
}

function tableStyle() {
    return {
        theme: 'plain' as const,
        styles: {
            fontSize: 8.5,
            textColor: INK,
            cellPadding: { top: 3, bottom: 3, left: 4, right: 4 },
            lineColor: LIGHT,
            lineWidth: { bottom: 0.75 },
            overflow: 'linebreak' as const,
        },
        headStyles: {
            fontStyle: 'bold' as const,
            lineColor: INK,
            lineWidth: { bottom: 1.25 },
        },
    }
}

function rowTitle(item: ReportItem, t: Translate): string {
    const detail = item.kind === 'filler' ? itemDetail(item, t) : ''

    return [itemTitle(item, t), detail].filter(Boolean).join(' · ')
}
