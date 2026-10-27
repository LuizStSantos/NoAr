import { i18n } from '@/i18n'
import { useAuth } from '@/stores/auth'

type AuthResponseType = 'json' | 'text' | 'blob' | 'arrayBuffer'

export type AuthFetchOptions = RequestInit & {
    responseType?: AuthResponseType
}

export class AuthFetchError<T = unknown> extends Error {
    constructor(
        public readonly response: Response,
        public readonly data: T,
    ) {
        super(fetchErrorMessage(response, data))
    }

    /** HTTP status of the failed response. */
    get status(): number {
        return this.response.status
    }
}

/**
 * The server's message for a body it could not read, such as
 * "Failed to deserialize the JSON body into the target type: scte35.pid:
 * invalid value: integer `99999`, expected u16 at line 1 column 2528", told
 * in the interface's language with the field it is about.
 */
function invalidBodyMessage(text: string): string | undefined {
    const match =
        /Failed to deserialize the JSON body into the target type: ([^:]+): (.+?)(?: at line \d+ column \d+)?$/s.exec(
            text,
        )

    if (!match) {
        return undefined
    }

    const value = /invalid value: \w+ `([^`]*)`/.exec(match[2]!)?.[1]

    return value === undefined
        ? i18n.t('error.invalidField', { field: match[1] })
        : i18n.t('error.invalidValue', { field: match[1], value })
}

function fetchErrorMessage(response: Response, data: unknown): string {
    if (typeof data === 'string' && data) {
        return (response.status === 400 || response.status === 422 ? invalidBodyMessage(data) : undefined) ?? data
    }
    if (data && typeof data === 'object') {
        const detail = 'detail' in data && typeof data.detail === 'string' ? data.detail : undefined
        const error = 'error' in data && typeof data.error === 'string' ? data.error : undefined

        if (detail || error) {
            return detail ?? error ?? `Request failed with status ${response.status}`
        }
    }

    return `Request failed with status ${response.status}`
}

/**
 * Sends an authenticated request and returns its parsed response body.
 */
export async function authFetch<T = unknown>(
    input: RequestInfo | URL,
    { responseType, ...init }: AuthFetchOptions = {},
): Promise<T> {
    const response = await authFetchRaw(input, init)
    const type = responseType ?? defaultResponseType(response)

    if (!response.ok) {
        // Error bodies follow their own content type, whatever the caller expects on success.
        throw new AuthFetchError(response, await readResponse(response, defaultResponseType(response)))
    }
    if (response.status === 204) {
        return undefined as T
    }

    return (await readResponse(response, type)) as T
}

/**
 * Sends an authenticated request and returns the unparsed response. Use this
 * for downloads and callers that need headers or streaming response bodies.
 */
export async function authFetchRaw(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
    const auth = useAuth()

    await auth.inspectToken()
    if (!auth.isLogin) {
        return new Response(null, { status: 401, statusText: 'Authentication required' })
    }

    const request = new Request(input, init)
    const send = async () => {
        const headers = new Headers(request.headers)
        headers.set('Authorization', `Bearer ${auth.jwtToken}`)

        return fetch(new Request(request.clone(), { headers }))
    }

    const accessToken = auth.jwtToken
    let response = await send()
    if (response.status !== 401) {
        return response
    }

    // Another request may have completed the shared refresh while this one
    // was in flight. In that case retry with its already-current access token.
    if (auth.jwtToken === accessToken && !(await auth.refreshToken())) {
        // The refresh token was rejected: the session is over (401). Else the
        // refresh only could not be done now (rate limit, server down): say
        // so with a 503, so callers show an error instead of logging out.
        return auth.jwtRefresh
            ? new Response(null, { status: 503, statusText: 'Session refresh delayed; try again' })
            : response
    }

    response = await send()
    return response
}

function defaultResponseType(response: Response): AuthResponseType {
    return response.headers.get('content-type')?.includes('application/json') ? 'json' : 'text'
}

async function readResponse(response: Response, type: AuthResponseType): Promise<unknown> {
    switch (type) {
        case 'json':
            return response.json()
        case 'blob':
            return response.blob()
        case 'arrayBuffer':
            return response.arrayBuffer()
        case 'text':
            return response.text()
    }
}
