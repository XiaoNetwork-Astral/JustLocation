#!/system/bin/sh
MODDIR=${0%/*}
# Configuration is retained so reinstalling does not discard saved coordinates.
"$MODDIR/bin/justlocationd" request eyJ2ZXJzaW9uIjoxLCJvcCI6InNodXRkb3duIn0= >/dev/null 2>&1
exit 0
