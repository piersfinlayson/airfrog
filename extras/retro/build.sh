PETCAT="petcat"
C1541="c1541"

AF_BAS="af-c64.bas"
AF_PRG="af-c64.prg"
AF_OUT="afc64"
DISK_HDR="piers.rocks,01"
DISK_NAME="airfrog.d64"

rm *.prg *.d64 2> /dev/null
$PETCAT -w2 -o $AF_PRG -- $AF_BAS
$C1541 -format "$DISK_HDR" d64 $DISK_NAME -write $AF_PRG $AF_OUT > /dev/null

echo "Created disk image: $DISK_NAME"