#! /bin/bash

/usr/local/bin/crank \
    consume-events \
    --coin-wallet "$COINWALLET" \
    --pc-wallet "$PCWALLET" \
    -d 9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin \
    -e "$EVENTSPERWORKER" \
    --log-directory "$LOGDIR" \
    -m "$MARKET"  \
    --num-accounts "$NUMACCOUNTS" \
    --num-workers "$NUMWORKERS" \
    --payer /tmp/payer.json
