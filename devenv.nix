{ pkgs, ... }:

{

  # https://devenv.sh/packages/
  packages = [ pkgs.just pkgs.openssl ];


  # https://devenv.sh/languages/
  languages.rust.enable = true;

  # https://devenv.sh/pre-commit-hooks/

  # https://devenv.sh/processes/

}
