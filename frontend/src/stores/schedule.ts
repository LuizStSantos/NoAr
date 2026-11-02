import { defineStore } from 'pinia'

import { authFetch } from '@/composables/authFetch'
import { useAuth } from './auth'
import { useConfig } from './config'

function channelId(): number | undefined {
    const configStore = useConfig()

    return configStore.channels[configStore.i]?.id
}

function headers(json = false): HeadersInit {
    const authStore = useAuth()
    const configStore = useConfig()

    return json ? { ...configStore.contentType, ...authStore.authHeader } : authStore.authHeader
}

export const useSchedule = defineStore('schedule', {
    state: () => ({
        date: '',
        occurrences: [] as ScheduleOccurrence[],
        program: [] as ProgramItem[],
        grade: [] as GuideBlock[],
        lives: [] as LiveSource[],
        graphics: [] as GraphicsEvent[],
        status: { active: null, queued: [], next: [] } as ScheduleStatus,
        loading: false,
    }),

    getters: {},
    actions: {
        async loadDay(date: string) {
            const id = channelId()

            if (!id) {
                return
            }

            this.date = date
            const [day, program, grade] = await Promise.all([
                authFetch<{ date: string; occurrences: ScheduleOccurrence[] }>(`/api/schedule/${id}?date=${date}`, {
                    headers: headers(),
                }),
                authFetch<ProgramItem[]>(
                    `/api/program/${id}?start_after=${date}T00:00:00&start_before=${date}T23:59:59`,
                    { headers: headers() },
                ).catch(() => [] as ProgramItem[]),
                authFetch<GuideBlock[]>(`/api/schedule/${id}/grade?date=${date}`, { headers: headers() }).catch(
                    () => [] as GuideBlock[],
                ),
            ])

            if (this.date === date) {
                this.occurrences = day.occurrences
                this.program = program ?? []
                this.grade = grade ?? []
            }
        },

        async loadGraphics() {
            const id = channelId()

            if (id) {
                this.graphics = await authFetch<GraphicsEvent[]>(`/api/schedule/${id}/graphics`, {
                    headers: headers(),
                })
            }
        },

        async saveGraphics(input: GraphicsEventInput, eventId?: number): Promise<GraphicsEvent> {
            const id = channelId()

            return authFetch<GraphicsEvent>(
                eventId ? `/api/schedule/${id}/graphics/${eventId}` : `/api/schedule/${id}/graphics`,
                { method: eventId ? 'PUT' : 'POST', headers: headers(true), body: JSON.stringify(input) },
            )
        },

        async deleteGraphics(eventId: number) {
            await authFetch(`/api/schedule/${channelId()}/graphics/${eventId}`, {
                method: 'DELETE',
                headers: headers(),
                responseType: 'text',
            })
        },

        async loadLives() {
            const id = channelId()

            if (id) {
                this.lives = await authFetch<LiveSource[]>(`/api/schedule/${id}/lives`, { headers: headers() })
            }
        },

        async loadStatus() {
            const id = channelId()

            if (id) {
                this.status = await authFetch<ScheduleStatus>(`/api/schedule/${id}/status`, { headers: headers() })
            }
        },

        async saveEvent(input: ScheduleEventInput, eventId?: number): Promise<ScheduleEvent> {
            const id = channelId()
            const url = eventId ? `/api/schedule/${id}/${eventId}` : `/api/schedule/${id}`

            return authFetch<ScheduleEvent>(url, {
                method: eventId ? 'PUT' : 'POST',
                headers: headers(true),
                body: JSON.stringify(input),
            })
        },

        async deleteEvent(eventId: number) {
            await authFetch(`/api/schedule/${channelId()}/${eventId}`, {
                method: 'DELETE',
                headers: headers(),
                responseType: 'text',
            })
        },

        async trigger(eventId: number) {
            await authFetch(`/api/schedule/${channelId()}/${eventId}/trigger`, {
                method: 'POST',
                headers: headers(),
                responseType: 'text',
            })
        },

        /** Push the live on air later: by `minutes`, or `until` a time (HH:MM), this airing only. */
        async extendLive(change: { minutes: number } | { until: string }): Promise<ActiveScheduleEvent> {
            return authFetch<ActiveScheduleEvent>(`/api/schedule/${channelId()}/extend`, {
                method: 'POST',
                headers: headers(true),
                body: JSON.stringify(change),
            })
        },

        async stop(): Promise<boolean> {
            return authFetch<boolean>(`/api/schedule/${channelId()}/stop`, {
                method: 'POST',
                headers: headers(),
            })
        },

        async saveLive(name: string, url: string, guide: LiveGuide): Promise<LiveSource> {
            return authFetch<LiveSource>(`/api/schedule/${channelId()}/lives`, {
                method: 'POST',
                headers: headers(true),
                body: JSON.stringify({ name, url, guide }),
            })
        },

        /** Change the address of an existing live; its guide data stays. */
        async updateLive(path: string, url: string): Promise<LiveSource> {
            return authFetch<LiveSource>(`/api/schedule/${channelId()}/lives`, {
                method: 'PUT',
                headers: headers(true),
                body: JSON.stringify({ path, url }),
            })
        },

        /** An assembled program (`.program` file in the storage). */
        async getProgram(path: string): Promise<AssembledProgram> {
            return authFetch<AssembledProgram>(
                `/api/schedule/${channelId()}/programs?path=${encodeURIComponent(path)}`,
                { headers: headers(false) },
            )
        },

        /** Save an assembled program; the clip lengths are read by the server. */
        async saveProgram(
            path: string,
            title: string,
            segments: (
                | { kind: 'block'; source: string }
                | { kind: 'break'; clips: string[] }
                | { kind: 'live'; source: string; duration: number }
            )[],
        ): Promise<AssembledProgram> {
            return authFetch<AssembledProgram>(`/api/schedule/${channelId()}/programs`, {
                method: 'PUT',
                headers: headers(true),
                body: JSON.stringify({ path, title, segments }),
            })
        },

        /** NDI sources announced on the network, by the name receivers list. */
        async ndiSources(): Promise<string[]> {
            return authFetch<string[]>(`/api/schedule/${channelId()}/lives/ndi`, {
                headers: headers(false),
            })
        },

        async testSignal(url: string): Promise<SignalTest> {
            return authFetch<SignalTest>(`/api/schedule/${channelId()}/lives/test`, {
                method: 'POST',
                headers: headers(true),
                body: JSON.stringify({ url }),
            })
        },
    },
})
