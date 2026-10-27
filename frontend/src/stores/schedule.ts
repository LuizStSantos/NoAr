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

        async testSignal(url: string): Promise<SignalTest> {
            return authFetch<SignalTest>(`/api/schedule/${channelId()}/lives/test`, {
                method: 'POST',
                headers: headers(true),
                body: JSON.stringify({ url }),
            })
        },
    },
})
