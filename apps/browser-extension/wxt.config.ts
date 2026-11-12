import { defineConfig } from "wxt";

// Public half of the dev signing key. It pins the Chromium id for unpacked dev builds only.
// The private key lives outside the repo (never commit it).
const DEV_KEY =
  "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAt/+NXpFKShHtSL7xU+EEHN+zG6RZjVwO7FDztoEhVDEfMako2D2n0WwOClpF2a3EuTbS7wgSg91BXUahfOZeiWE6J8viV5k+rOki6v0vtZWtx/G7/1N3OMJSMWl67IcXTlTRLzMOOaPokzDPM6TDV4ZhnU/pj3zjE6GLPiz8ZQUhAR5wCXVnbWA+AwOM+zaM6gzxnnr5j+8FaFERg0N/Dna9mi99/SI7hsb9pDEmztC/BX5q4WPRIYlMmEdmRpHclt3RttZdoM5fYhhX6onDhtzPQRApc1CgETcGs4resn8fQUhjvxKNi2DiRuYeI4ZT8tHJfeR/Lks/2pZMBsP4AQIDAQAB";

export default defineConfig({
  srcDir: "src",
  zip: {
    artifactTemplate: "screenbound-companion-{{version}}-{{browser}}.zip",
    sourcesTemplate: "screenbound-companion-{{version}}-sources.zip",
  },
  modules: ["@wxt-dev/module-svelte"],
  manifest: ({ mode }) => ({
    name: "Screenbound Companion",
    description:
      "Companion add-on for Screenbound (also works with Desqu): fullscreen video stays in its window or moves into a zone, without the flash.",
    permissions: ["storage", "tabs", "nativeMessaging"],
    host_permissions: ["<all_urls>"],
    browser_specific_settings: {
      gecko: {
        id: "desqu-fullscreen@wolfskii.github.io",
        strict_min_version: "128.0",
        // Nothing is collected or sent off the machine (native messaging is local only).
        data_collection_permissions: { required: ["none"] },
      },
    },
    ...(mode === "development" ? { key: DEV_KEY } : {}),
  }),
});
