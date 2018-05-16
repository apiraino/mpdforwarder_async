## MPD forwarder (async version)

 A simple proxy that forwards TCP payload to an MPD server.

 This is the async version, using the tokio library.

A pet project to give a first glance at the Tokio library (basically some copy and paste from the tutorial).

### Run with:

    $ cargo watch -x "run 1"

### Send cmd with:

    $ echo "status" | ncat --send-only 127.0.0.1 6601

    `--send-only` to avoid the client->proxy socket half to hang in TIME_WAIT

### Check what the server receives:

    $ while true; do ncat -l 6600 ; done

### Check MPD log

    $ tail -f ~/.mpd/mpd.log

### Use telnet

    $ telnet localhost 6601
    status
    <CTRL + ]>
    telnet> quit
