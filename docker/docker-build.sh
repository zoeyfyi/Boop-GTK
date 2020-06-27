# Builds docker containers for cross compilation

SCRIPT=`realpath $0`
SCRIPTPATH=`dirname $SCRIPT`

docker build -t boop-gtk/aarch64-unknown-linux-gnu -f $SCRIPTPATH/aarch64-unknown-linux-gnu.Dockerfile $SCRIPTPATH
docker build -t boop-gtk/x86_64-unknown-linux-gnu -f $SCRIPTPATH/x86_64-unknown-linux-gnu.Dockerfile $SCRIPTPATH
docker build -t boop-gtk/x86_64-pc-windows-gnu -f $SCRIPTPATH/x86_64-pc-windows-gnu.Dockerfile $SCRIPTPATH
