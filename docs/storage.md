# Storage: uploading and downloading files

Storage shows the configured media directory of the selected channel.
Use **Upload files** or drag files into the current folder. Existing upload
collision checks remain in place; downloading does not replace server files.

To save a server file on your computer:

1. Open **Storage** and the folder containing the file.
2. Choose **Videos**, **Images** or **All files**. WebVTT subtitle files appear
   under **All files**.
3. Click the **Download** arrow in the file's Actions column, or on an image
   card. The image viewer also has a **Download** button.
4. The browser saves the file using its server filename and your browser's
   normal download settings. On a narrow screen, the table scrolls horizontally
   to reveal its Actions column.

This downloads one file at a time, exactly as currently stored. Downloading does
not transcode or re-encode it, and a large video is streamed by the browser
rather than collected into an application-sized JavaScript buffer. If a file
was converted earlier, the stored converted file is what is downloaded.
Folder rows have no download action.

The file picker accepts `.vtt` alongside configured media extensions, images and
`.nfo` sheets. Upload a subtitle beside the video with the same base name; see
[Closed captions](closed_captions.md) for its format and output settings.
