#! /bin/bash
docker-compose logs > logs
LOGS=$(grep -i "processed .* instructions for" logs)
HIGHEST_MARKET_COUNT=$(echo "$LOGS" | awk -F 'markets:' '{print $1}' | awk '{print $NF}'  | sort -u | tail -n 1)
CRANK_COUNT=$(echo "$LOGS" | awk -F 'markets:' '{print $1}' | wc -l)
echo "found records of $CRANK_COUNT cranks, with highest markets in tx $HIGHEST_MARKET_COUNT"
ERRORS=$(echo "$LOGS" | grep -i "error")
FAILS=$(echo "$LOGS" | grep -i "fail")

if [[ "$ERRORS" != "" ]]; then
        echo "found possible errors"
        echo "$ERRORS"
fi

if [[ "$FAILS" != "" ]]; then
        echo "found possible failures"
        echo "$FAILS"
fi

CRANK_TXS=$(grep -i "crank ran" logs | awk -F 'crank ran' '{print $2}' | awk '{print $1}')
FIVE_TXS=$(grep -i "crank ran" logs | awk -F 'crank ran' '{print $2}' | awk '{print $1}' | tail -n 5)
echo "5 most recent crank transactions"
echo "$FIVE_TXS"
