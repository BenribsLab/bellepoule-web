# Écrit la directive nginx Content-Security-Policy du client : les scripts
# en ligne de index.html (chargeur généré par trunk) sont autorisés par leur
# empreinte SHA-256, aucun autre.
use strict;
use warnings;
use Digest::SHA qw(sha256_base64);

local $/;
my $html = <>;
my @hashes;
while ($html =~ /<script(?![^>]*\bsrc=)[^>]*>(.*?)<\/script>/sg) {
    my $digest = sha256_base64($1);
    $digest .= "=" x ((4 - length($digest) % 4) % 4);
    push @hashes, "'sha256-$digest'";
}
my $policy = join "; ",
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval' @hashes",
    # Styles en ligne : positions calculées par l'application (attributs style).
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'";
print "add_header Content-Security-Policy \"$policy\" always;\n";
