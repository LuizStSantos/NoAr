**Playlist generation**
================

**Player › Generate** fills one or more days from the folders you pick, or
block by block at set times, as a draft you review before saving.

The same can be done from a template, through the API or the command line. A
template could look like this:

```JSON
{
    "sources": [
        {
            "start": "00:00:00",
            "duration": "02:00:00",
            "shuffle": true,
            "paths": [
                "/path/to/folder/1"
            ]
        },
        {
            "start": "02:00:00",
            "duration": "04:00:00",
            "shuffle": false,
            "paths": [
                "/path/to/folder/2",
                "/path/to/folder/3",
                "/path/to/folder/4"
            ]
        },
        {
            "start": "06:00:00",
            "duration": "10:00:00",
            "shuffle": true,
            "paths": [
                "/path/to/folder/5"
            ]
        },
        {
            "start": "16:00:00",
            "duration": "06:00:00",
            "shuffle": false,
            "paths": [
                "/path/to/folder/6",
                "/path/to/folder/7"
            ]
        },
        {
            "start": "22:00:00",
            "duration": "02:00:00",
            "shuffle": true,
            "paths": [
                "/path/to/folder/8"
            ]
        }
    ]
}
```

Saved as a file in `./data`, it can be run in the container:

```BASH
docker exec noar noar -g 2026-09-04 - 2026-09-10 --template /data/playlist_template.json
```

Or through API:

```BASH
curl -X POST http://127.0.0.1:8787/api/playlist/1/generate/2026-09-05
    -H 'Content-Type: application/json' -H 'Authorization: Bearer <TOKEN>'
    --data '{"template": {"sources": [\
                {"start": "00:00:00", "duration": "10:00:00", "shuffle": true, "paths": ["path/1", "path/2"]}, \
                {"start": "10:00:00", "duration": "14:00:00", "shuffle": false, "paths": ["path/3", "path/4"]}]}}'
```
