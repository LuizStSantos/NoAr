**Guia de programação XMLTV**
================

O ffplayout gera um guia XMLTV (EPG) a partir das playlists do canal e dos
arquivos `.nfo` colocados na pasta de cada programa. As imagens das pastas
(pôster, fanart, miniatura do episódio) entram no guia como links, servidos
pelo próprio ffplayout.

O guia é público, como os arquivos HLS, para que players IPTV (Kodi, Jellyfin,
TiviMate, Plex etc.) possam baixá-lo sem login.

```text
http://<servidor>:8787/xmltv/<id-do-canal>/guide.xml
```

## Organização das pastas

Cada programa tem sua pasta dentro do storage do canal, com um `.nfo` no
formato do Kodi:

```text
storage/
  Jornal da Manhã/
    tvshow.nfo                 <- dados do programa
    poster.jpg                 <- pôster do programa
    fanart.jpg                 <- imagem larga (fundo)
    bloco 1.mp4
    bloco 2.mp4
  Novela Amor de Verão/
    programa.nfo
    poster.png
    Temporada 1/               <- subpastas herdam o .nfo do programa
      Capítulo 01.mp4
      Capítulo 01.nfo          <- opcional: dados do episódio
      Capítulo 01-thumb.jpg    <- opcional: imagem do episódio
  Filmes/
    O Grande Filme (2019)/
      movie.nfo
      poster.jpg
      filme.mp4
```

### O `.nfo` do programa

O ffplayout procura o `.nfo` do programa na pasta do vídeo e depois em cada
pasta acima, até chegar (sem incluir) à raiz do storage. Nomes preferidos, nesta
ordem: `tvshow.nfo`, `programa.nfo`, `program.nfo`, `show.nfo`, `movie.nfo`,
`folder.nfo`. Se nenhum existir, vale qualquer outro `.nfo` da pasta que não
tenha o mesmo nome de um vídeo.

```xml
<tvshow>
  <title>Jornal da Manhã</title>
  <plot>As principais notícias do dia.</plot>
  <genre>Jornalismo</genre>
  <mpaa>BR:L</mpaa>
  <premiered>2019-03-04</premiered>
  <actor><name>Ana Souza</name><role>Apresentadora</role></actor>
  <thumb aspect="poster">poster.jpg</thumb>
</tvshow>
```

### O `.nfo` do episódio (opcional)

Um `.nfo` com o mesmo nome do vídeo descreve só aquele vídeo. O título dele vira
o subtítulo no guia, e a sinopse, a data, a temporada e o episódio dele têm
prioridade sobre os do programa.

```xml
<episodedetails>
  <title>O Reencontro</title>
  <season>1</season>
  <episode>1</episode>
  <aired>2026-10-14</aired>
  <plot>Helena volta à cidade depois de dez anos.</plot>
</episodedetails>
```

Se não houver `.nfo` de programa, o `.nfo` do vídeo é usado como o próprio
programa (útil para um filme ao lado de `nome-do-filme.nfo`), a menos que seja
um `<episodedetails>`.

### Campos lidos

| Campo do `.nfo` | No XMLTV |
| --- | --- |
| `title` | `title` (programa) ou `sub-title` (episódio) |
| `showtitle` | `title`, quando só existe o `.nfo` do episódio |
| `plot`, `outline` | `desc` |
| `genre` | `category` (programa e episódio somados) |
| `premiered`, `aired`, `releasedate`, `year` | `date` |
| `season`, `episode` | `episode-num` (`xmltv_ns` e `onscreen`, ex. `S01E02`) |
| `mpaa`, `certification` | `rating`; `BR:14` vira sistema `BR`, valor `14` |
| `director`, `actor`, `credits` | `credits` |
| `country` | `country` |
| `thumb`, `fanart` | imagens, quando não há arquivo de imagem na pasta |

Arquivos em UTF-8, UTF-16 e Windows-1252 (comum em arquivos salvos no Bloco de
Notas) são lidos corretamente. Um `.nfo` quebrado ou que só contém um link não
derruba o guia: vale o que puder ser lido.

### Imagens

Extensões aceitas: `jpg`, `jpeg`, `png`, `webp`, `gif`.

| Tipo | Arquivos procurados |
| --- | --- |
| Pôster do programa | `poster`, `folder`, `cover`, `capa`, `show` |
| Imagem larga do programa | `landscape`, `thumb`, `fanart`, `backdrop`, `background` |
| Imagem do episódio | `<vídeo>-thumb`, `<vídeo>` |
| Pôster de um filme avulso | `<vídeo>-poster`, `<vídeo>` |

O `<icon>` de cada programa no guia usa a imagem do episódio, depois o pôster,
depois a imagem larga. Todas também saem como `<image type="poster|backdrop|still">`.
Um `<thumb>` com endereço `http(s)://` é usado como está.

O logo do canal (configuração de logo da saída) vira o ícone do canal no guia.

## Como o guia é montado

- Os horários seguem a playlist, como na tela de programação do ffplayout.
- Vídeos seguidos do mesmo programa viram uma única entrada no guia (ex. os
  blocos de um telejornal). Episódios com `.nfo` próprio ficam separados.
- Comerciais (`"ad": true` na playlist) e o filler configurado não aparecem:
  o tempo deles é somado ao programa que interrompem.
- Sem `.nfo`, o título vem do título do item na playlist ou do nome do arquivo.

## Parâmetros

Todos opcionais:

| Parâmetro | Padrão | Uso |
| --- | --- | --- |
| `days` | `7` | Dias a partir do início de hoje (1 a 14). |
| `lang` | — | Idioma de títulos e descrições, ex. `pt` ou `pt-BR`. |
| `tvg_id` | `ffplayout.<id>` | Id do canal no guia; deve ser igual ao `tvg-id` da lista M3U. |
| `base_url` | endereço da requisição | Endereço público para os links das imagens, quando há proxy reverso em subpasta. |

Exemplo:

```text
https://tv.exemplo.com.br/xmltv/1/guide.xml?days=3&lang=pt&tvg_id=TVExemplo.br
```

E na lista M3U do player:

```text
#EXTM3U url-tvg="https://tv.exemplo.com.br/xmltv/1/guide.xml?tvg_id=TVExemplo.br"
#EXTINF:-1 tvg-id="TVExemplo.br",TV Exemplo
https://tv.exemplo.com.br/public/1/live/stream.m3u8
```

Atrás de nginx ou outro proxy, envie `X-Forwarded-Proto` e `X-Forwarded-Host`
para que os links das imagens usem o endereço público.

## Imagens no servidor

```text
/xmltv/<id-do-canal>/images/<caminho-dentro-do-storage>
```

Só arquivos de imagem dentro do storage do canal são servidos; vídeos, `.nfo`
e caminhos que saem do storage (inclusive por links simbólicos) são recusados.
