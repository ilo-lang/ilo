double() { echo $(( $1 * 2 )); }
quad() { echo "$(double "$(double "$1")")"; }
quad 7
