/* Closing one FFmpeg-style SRT session must not tear down another session. */
#include <srt/srt.h>
#include <arpa/inet.h>
#include <stdio.h>
#include <string.h>

static int failed(const char *operation)
{
    fprintf(stderr, "FAIL: %s: %s\n", operation, srt_getlasterror_str());
    return 1;
}

int main(void)
{
    if (srt_startup() < 0)
        return failed("startup");

    SRTSOCKET listener = srt_create_socket();
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (srt_bind(listener, (struct sockaddr *)&address, sizeof(address)) < 0 ||
        srt_listen(listener, 1) < 0)
        return failed("listen");
    int length = sizeof(address);
    if (srt_getsockname(listener, (struct sockaddr *)&address, &length) < 0)
        return failed("getsockname");

    SRTSOCKET sender = srt_create_socket();
    int timeout_ms = 2000;
    srt_setsockflag(sender, SRTO_SNDTIMEO, &timeout_ms, sizeof(timeout_ms));
    srt_setsockflag(sender, SRTO_CONNTIMEO, &timeout_ms, sizeof(timeout_ms));
    if (srt_connect(sender, (struct sockaddr *)&address, sizeof(address)) < 0)
        return failed("connect");
    SRTSOCKET receiver = srt_accept(listener, NULL, NULL);
    if (receiver == SRT_INVALID_SOCK)
        return failed("accept");
    srt_setsockflag(receiver, SRTO_RCVTIMEO, &timeout_ms, sizeof(timeout_ms));

    for (int cycle = 0; cycle < 20; ++cycle) {
        if (srt_startup() < 0)
            return failed("auxiliary startup");
        SRTSOCKET auxiliary = srt_create_socket();
        if (auxiliary == SRT_INVALID_SOCK || srt_close(auxiliary) < 0)
            return failed("auxiliary close");
        if (srt_cleanup() < 0)
            return failed("auxiliary cleanup");

        const char payload[] = "persistent-output";
        char received[1316];
        if (srt_sendmsg(sender, payload, sizeof(payload), -1, 1) != (int)sizeof(payload))
            return failed("send after auxiliary cleanup");
        int count = srt_recvmsg(receiver, received, sizeof(received));
        if (count != (int)sizeof(payload) || memcmp(payload, received, sizeof(payload)) != 0)
            return failed("receive after auxiliary cleanup");
    }

    srt_close(receiver);
    srt_close(sender);
    srt_close(listener);
    srt_cleanup();
    puts("PASS: 20 auxiliary sessions closed without interrupting the persistent connection");
    return 0;
}
