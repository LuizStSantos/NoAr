# Test the graphics branch on a disposable machine

This procedure replaces the running stable instance on a test machine. It
starts an empty installation on the same web port, without copying the stable
database or requiring a manual backup. Existing channels, media, playlists and
accounts are not imported. Use Docker Engine/Desktop with Compose v2 and Git.
Commands below also work in a terminal with Docker Desktop; run them where
Docker can build Linux containers.

## Stop the stable instance

In the directory containing the Compose file used by the running stable
installation:

```sh
docker compose stop noar
```

If it was installed without Compose, stop its container/service using that
installation's normal method. Port 8787 must be free before starting the test.
This step only stops the existing instance; it does not upgrade its database.

## Build and start this branch

Use a new checkout, so local edits to the stable installation do not enter the
build:

```sh
git clone --single-branch --branch claude/branch-review-opz967 https://github.com/LuizStSantos/NoAr.git NoAr-graphics-test
cd NoAr-graphics-test
git log -1 --oneline
docker compose -f docker-compose.graphics-test.yml build
docker compose -f docker-compose.graphics-test.yml up -d --no-build
docker compose -f docker-compose.graphics-test.yml logs -f noar
```

The first build compiles the web interface, Rust application and native media
libraries; it can take a while and needs free disk space. Wait for the build
to succeed before starting. Ctrl+C exits log following without stopping NoAr.

Open **http://localhost:8787** on this machine, or
**http://TEST_MACHINE_IP:8787** from another device. Complete the setup page
and choose an administrator account. Keep the suggested `/data` paths. The
Compose project uses its own `graphics-test-data` volume and local image
`noar:graphics-review`. It does not pull `latest`, change the stable image tag
or enable the in-app updater/Watchtower for this container. Test devices on
the LAN need the machine's firewall to allow port 8787.

If the setup page does not appear, this test project already has saved data.
To intentionally erase **only this test project's volume** and start again:

```sh
docker compose -f docker-compose.graphics-test.yml down --volumes
docker compose -f docker-compose.graphics-test.yml up -d --no-build
```

## Check the graphics

1. Upload a short video and logo/artwork images in Storage. Set up the channel
   output as HLS with a software H.264 encoder, then start playout. The Compose
   file does not grant GPU, display or video-card devices.
2. In Graphics → Templates → Channel graphics, edit the channel logo in Composer.
   Select its image, resize/move it and enable Always on air. Save, show it
   through the library switch and check the actual Player/HLS picture.
3. Edit the L-shape model: add artwork and keep one video window. Select its
   corner and size, save, then use Enter L-shape / Return to full screen.
   Check the actual window and logo suppression during the transition.
4. Edit the ticker text model, enable Always on air and show it in Templates.
   Create an ordinary temporary template with a text field and test take/out
   from On air. A bound channel model uses its channel control instead of take.
5. In Graphics → XMLTV logo, choose another image, save and reload. Read the
   channel's XMLTV feed to confirm its independent icon.
6. In Graphics → External control (API), choose the channel/temporary model in
   the examples. The page uses real IDs and does not execute commands. The
   existing graphics key works for logo/L-shape/scenes; `/api/graphics` examples
   require Bash (for example WSL/Git Bash on Windows), curl 7.76+ and a login
   access token; the Always on air example also uses jq. Complete two-factor authentication if enabled.
   Copy the returned `access` token into the local `TOKEN` shell variable and
   replace example field values. Read actual output and `/health` after commands.
7. Restart this test instance and check that the saved models return with their
   configured startup visibility:

   ```sh
   docker compose -f docker-compose.graphics-test.yml restart noar
   ```

This fresh-install route tests authoring and playback. It does **not** exercise
conversion of an existing 2.2.1 database; that separate migration/rollback and
frame-parity proof is recorded in the [graphics handoff](graphics-handoff/README.md).

## Update or stop the test

To fetch later changes on the same branch and rebuild:

```sh
git pull --ff-only origin claude/branch-review-opz967
docker compose -f docker-compose.graphics-test.yml build
docker compose -f docker-compose.graphics-test.yml up -d --no-build
```

To stop/remove this test's containers:

```sh
docker compose -f docker-compose.graphics-test.yml down
```

Add `--volumes` only when you want to discard this test's data. To return to
the stable installation, stop the test, return to the stable installation's
original directory and start its own Compose/service. This test's migrated
volume is not a database for the stable binary; a clean stable installation is
also an option on a disposable machine.
