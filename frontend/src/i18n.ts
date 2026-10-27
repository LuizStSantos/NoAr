import { createI18n } from 'vue-i18n'

import enUS from './locales/en-US.ts'
import ptBR from './locales/pt-BR.ts'

export const locales = [
    {
        code: 'en',
        language: 'en-US',
        name: 'English',
    },
    {
        code: 'pt-br',
        language: 'pt-BR',
        name: 'Português (BR)',
    },
]

const instance = createI18n({
    legacy: false,
    locale: 'en-US',
    fallbackLocale: 'en',
    messages: {
        en: enUS,
        'pt-br': ptBR,
    },
})

export default instance

export const i18n = instance.global
