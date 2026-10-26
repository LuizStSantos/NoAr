import type { JwtPayload } from 'jwt-decode'
import type { PlayoutConfig, Playlist as Ply } from './playout_config'

export {}

declare global {
    interface JwtPayloadExt extends JwtPayload {
        id: number
        channels: number[]
        role: string
        token_type: 'access' | 'refresh'
    }

    interface PlaylistExt extends Ply {
        startInSec: number
        lengthInSec: number
    }

    interface PlayoutConfigExt extends PlayoutConfig {
        playlist: PlaylistExt
    }

    interface PlayoutOutput {
        id: number
        name: string
        hls_variants: string
        stream_url: string
        stream_type: 'rtmp' | 'srt' | 'udp' | 'custom' | null
        stream_format: string | null
        hls_playlist_name: string | null
        hls_segment_duration: number | null
        hls_list_size: number | null
        desktop_fullscreen: boolean
        width: number
        height: number
        fps: number
        video_codec: string | null
        video_options: string
        protocol_options: string
        muxer_options: string
        metadata_options: string
        audio_codec: string | null
        audio_options: string
        audio_bitrate: number | null
        channel_id: number
    }

    interface CodecOption {
        name: string
        display_name: string
        codec_id: string
        hardware: boolean
        uses_bitrate: boolean
        settings: EncoderSetting[]
    }

    interface EncoderSetting {
        key: string
        label: string
        kind: 'select' | 'number'
        default: string
        choices: EncoderSettingChoice[]
        minimum: number | null
        maximum: number | null
        visible_when: EncoderSettingVisibility | null
    }

    interface EncoderSettingChoice {
        value: string
        label: string
    }

    interface EncoderSettingVisibility {
        key: string
        value: string
    }

    interface OutputCodecOptions {
        video: CodecOption[]
        audio: CodecOption[]
    }

    interface PlayoutCodecOptions {
        hls: OutputCodecOptions
        rtmp: OutputCodecOptions
        srt: OutputCodecOptions
        udp: OutputCodecOptions
        custom: OutputCodecOptions
        recording: OutputCodecOptions
    }

    interface Token {
        access: string
        refresh: string
    }

    interface DataAuth {
        uuid: string
    }

    interface Channel {
        id: number
        extra_extensions: string | string[]
        name: string
        preview_url: string
        public: string
        playlists: string
        storage: string
        timezone?: string
        active?: boolean
        last_date?: string | null
        time_shift?: number
    }

    interface GlobalSettings {
        smtp_server: string
        smtp_user: string
        smtp_password_set: boolean
        smtp_starttls: boolean
        smtp_port: number
        notification_server: string
        notification_token_set: boolean
    }

    interface User {
        id: number
        username: string
        mail?: string
        password?: string
        confirm?: string
        admin?: boolean
        channel_ids?: number[]
        role_id?: number
        two_factor: boolean
    }

    interface Crumb {
        text: string
        path: string
    }

    interface Payload {
        method: string
        headers: any
        body?: any
    }

    interface Playlist {
        channel: string
        date: string
        program: PlaylistItem[]
    }

    interface PlaylistItem {
        date?: string
        uid: string
        begin: number
        title?: string | null
        source: string
        audio?: string
        duration: number
        in: number
        out: number
        ad?: boolean
        custom_filter?: string
        overtime?: boolean
    }

    interface FileObject {
        name: string
        duration: number
        /** Size in bytes. */
        size?: number
        /** Last change, seconds since the Unix epoch. */
        modified?: number | null
        /** Whether the channel can play it. */
        media?: boolean
        /** Whether it has a sheet of its own (`<name>.nfo`). */
        sheet?: boolean
        /** Whether it keeps the logo off somewhere (`<name>.logo.json`). */
        logo_marks?: boolean
    }

    interface Folder {
        uid: string
        name: string
    }

    interface FileFolderObject {
        source: string
        parent: string
        parent_folders: Folder[]
        folders: Folder[]
        /** Subfolders with a program sheet of their own. */
        sheet_folders?: string[]
        /** The films among `sheet_folders`. */
        movie_folders?: string[]
        /** Subfolders holding a live's address file. */
        live_folders?: string[]
        /** Subfolders marked as commercials folders. */
        commercial_folders?: string[]
        /** The commercials folder the browsed folder lies in (itself or a parent). */
        commercial?: string | null
        files: FileObject[]
    }

    type SheetKind = 'program' | 'movie' | 'episode'
    type ArtworkKind = 'poster' | 'backdrop' | 'still'

    /** A person's job in the credits, in the order of the XMLTV DTD. */
    type CreditRole =
        | 'director'
        | 'actor'
        | 'writer'
        | 'adapter'
        | 'producer'
        | 'composer'
        | 'editor'
        | 'presenter'
        | 'commentator'
        | 'guest'

    /** One person in the credits of a sheet. */
    interface Credit {
        name: string
        role: CreditRole
        /** The character an actor plays. */
        character: string | null
        /** An actor's guest appearance. */
        guest: boolean
        /** The person's `<thumb>` in the `.nfo`, kept as written. */
        thumb: string | null
    }

    interface SheetFields {
        title: string
        plot: string
        genres: string[]
        rating: string
        year: string
        premiered: string
        countries: string[]
        credits: Credit[]
        season: number | null
        episode: number | null
    }

    /** The `.nfo` sheet of a program folder or a clip, with its artwork. */
    interface MediaSheet {
        clip: boolean
        kind: SheetKind
        /** Storage path of the `.nfo`, when there is one. */
        file: string | null
        /** Storage path of the folder whose program sheet applies here. */
        inherited_from: string | null
        fields: SheetFields
        raw: string
        poster: string | null
        backdrop: string | null
        still: string | null
        /** Storage path of the folder whose `.actors/` holds the people's photos. */
        photo_folder: string
        /** Photos there: photo key of a name (see `photoKey`) -> storage path. */
        photos: Record<string, string>
        /** For a clip: the program folder around it and the people of its sheet. */
        program: string | null
        program_credits: Credit[]
    }

    interface FolderObject {
        source: string
        parent: string
        folders: Folder[]
    }

    interface SourceObject {
        type: string
        src: string
    }

    interface TemplateItem {
        start: string
        duration: string
        shuffle: boolean
        paths: string[] | any[]
    }

    interface Template {
        sources: TemplateItem[]
    }

    interface BodyObject {
        paths?: string[]
        shuffle?: boolean
        template?: Template
    }

    interface SystemStatistics {
        cpu: { cores: number; usage: number }
        load: { one: number; five: number; fifteen: number }
        memory: { total: number; used: number; process: number }
        network?: { name: string; current_in: number; current_out: number; total_in: number; total_out: number }
        storage?: { path: string; total: number; free: number }
        swap: { total: number; used: number; free: number }
        system: { name?: string; kernel?: string; version?: string; ffp_version?: string }
    }

    interface PlayoutStatus {
        media: PlaylistItem
        index: number
        audio?: AudioLevel
        loudness?: LiveLoudnessMetrics
        ingest: boolean
        ingest_listener_id?: number | null
        ingest_listener_name?: string | null
        ingest_listener_backend?: string | null
        mode: string
        elapsed: number
        shift: number
        /** Seconds since the playout was started. */
        uptime?: number | null
        title?: string
        /** How a network output (SRT, RTMP, UDP) reaches its destination. */
        output_link?: OutputLinkStatus
    }

    interface OutputLinkStatus {
        state: 'idle' | 'connecting' | 'listening' | 'connected' | 'retrying'
        /** Address without query string or stream key. */
        destination: string | null
        error: string | null
        /** Seconds in the current state. */
        seconds: number | null
        connections: number
    }

    /** The guide programme a playlist clip belongs to. */
    interface ClipProgramme {
        key: string
        title: string
        sub_title: string | null
        is_break: boolean
        /** A short clip without a sheet (ident, promo) inside a program. */
        insert: boolean
        /** From a commercials folder: a break whatever the playlist says. */
        commercial?: boolean
    }

    interface AudioLevel {
        rms_db: number
        peak_db: number
    }

    interface LiveLoudnessMetrics {
        momentary_lufs: number | null
        short_term_lufs: number | null
        integrated_lufs: number | null
        true_peak_dbtp: number | null
        rider_gain_db: number
        limiter_gain_reduction_db: number
    }

    interface SplitTime {
        id: number
        val: number
    }

    declare namespace Intl {
        type Key = 'calendar' | 'collation' | 'currency' | 'numberingSystem' | 'timeZone' | 'unit'

        function supportedValuesOf(input: Key): string[]
    }
}
