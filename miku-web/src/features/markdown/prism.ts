import { refractor } from "refractor";
import docker from "refractor/docker";
import graphql from "refractor/graphql";
import hcl from "refractor/hcl";
import jsx from "refractor/jsx";
import nix from "refractor/nix";
import powershell from "refractor/powershell";
import toml from "refractor/toml";
import tsx from "refractor/tsx";
import zig from "refractor/zig";
import rehypePrismGenerator from "rehype-prism-plus/generator";

const extendedLanguages = [docker, graphql, hcl, jsx, nix, powershell, toml, tsx, zig];

for (const language of extendedLanguages) {
  refractor.register(language);
}

refractor.alias("hcl", "terraform");

export const rehypePrism = rehypePrismGenerator(refractor);

export const supportedPrismLanguages = new Set(refractor.listLanguages());
