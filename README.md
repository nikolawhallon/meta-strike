# meta-strike

A Twilio<->Deepgram<->Game integration. Allows you to call +1(734)802-2990
to speak with a Deepgram Voice Agent (VA). The VA can execute one function
call on your behalf - "strike" - when this function call is executed, it
sends the text message "STRIKE" down any websocket connection to the `/game`
endpoint. I use this in order to destroy all units and buildings in the
game Data Wars (https://github.com/nikolawhallon/data-wars).
