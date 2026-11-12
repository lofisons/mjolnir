// On Windows, give Mjolnir.exe its own icon and name. Done here in plain
// JavaScript so the Windows app can be built on any machine, with no Wine.
const fs = require("node:fs");
const path = require("node:path");

exports.default = async function afterPack(context) {
  if (context.electronPlatformName !== "win32") return;
  const ResEdit = await import("resedit");
  const PE = await import("pe-library");
  const info = context.packager.appInfo;
  const exePath = path.join(context.appOutDir, `${info.productFilename}.exe`);
  const exe = PE.NtExecutable.from(fs.readFileSync(exePath), { ignoreCert: true });
  const res = PE.NtExecutableResource.from(exe);

  const icon = ResEdit.Data.IconFile.from(fs.readFileSync(path.join(__dirname, "build", "icon.ico")));
  const group = ResEdit.Resource.IconGroupEntry.fromEntries(res.entries)[0];
  ResEdit.Resource.IconGroupEntry.replaceIconsForResource(
    res.entries,
    group ? group.id : 1,
    group ? group.lang : 1033,
    icon.icons.map((i) => i.data),
  );

  const version = ResEdit.Resource.VersionInfo.fromEntries(res.entries)[0];
  const [major, minor, patch] = info.version.split(".").map((n) => parseInt(n, 10) || 0);
  version.setFileVersion(major, minor, patch, 0, 1033);
  version.setProductVersion(major, minor, patch, 0, 1033);
  version.setStringValues(
    { lang: 1033, codepage: 1200 },
    {
      ProductName: info.productName,
      FileDescription: info.productName,
      CompanyName: "lofisons",
      LegalCopyright: "MIT, lofisons",
      OriginalFilename: `${info.productFilename}.exe`,
      InternalName: info.productName,
    },
  );
  version.outputToResourceEntries(res.entries);
  res.outputResource(exe);
  fs.writeFileSync(exePath, Buffer.from(exe.generate()));
};
