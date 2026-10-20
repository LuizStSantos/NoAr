export {}

declare global {
    interface UpdateChange {
        commit: string
        /** First line of the commit message. */
        title: string
    }

    interface UpdateRelease {
        commit: string
        /** When the image was published, Unix milliseconds. */
        published_at: number | null
        /** Commits since the installed one, newest first, up to 30. */
        changes: UpdateChange[]
        change_count: number | null
        /** GitHub page with the changes. */
        url: string
    }

    interface UpdateStatus {
        version: string
        /** Empty for a build without a commit, such as a local one. */
        commit: string | null
        /** When the installed commit was made, Unix milliseconds. */
        commit_time: number | null
        latest: UpdateRelease | null
        available: boolean
        /** Unix milliseconds of the last check. */
        checked_at: number | null
        error: string | null
        auto_check: boolean
        repository: string
    }
}
