# Player-supplied game files (oracle snapshot)

This file records the exact contents of the copy in `game/` used for Phase 0. `legacy/Elite3/` was verified byte-identical (71 files, same SHA-256). The files themselves are **not** part of this repository and are ignored by git.

> **Do not commit any of these files.** They are Frontier Developments / Gametek copyrighted material. See `docs/LEGAL.md`.

## Identification

- Executable self-identification string: `English Release 1.02 - 21/04/95`.
- PE image: 32-bit x86, 5 sections (`CODE`, `DATA`, `.idata`, `.edata`, `.reloc`), image base `0x00400000`.
- Installer metadata (`BLD_INFO.CFG`, decoded by subtracting 1 from every byte): build directory `c:\games\firstenc\patches\eng-fp`, product title `FIRST ENCOUNTERS`, disk filename `GAMETEK`, total bytes 2052.00 KiB.
- `IMAGE.DAT` is the installer manifest: `FIRSTENC.EXE`, compressed size 739,463, installed size 2,101,248 (2 MiB + 4 KiB). `GAMETEK.1` (739,463 bytes) is the compressed installer payload for the executable and is not read by the game at runtime.
- `PATCHES.INI` is the AIL/UltraSound patch-map configuration, **not** a First Encounters gameplay patch.
- Version key for `data.bin`: `ffe-floppy-1.02-en` (provisional; derived from the release string and hash below).

### Primary hash

```
FIRSTENC.EXE
  size    2,097,152 bytes
  sha256  d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72
```

### Runtime files

`FIRSTENC.EXE`, `FX/*.RAW`, `MUSIC/*.HMP`, `CABIN0.BMP`..`CABIN5.BMP`, `COMMANDR/` (save directory), and optionally the sound configuration files. `GAMETEK.1`, `IMAGE.DAT`, `EDPMI.SWP`, `32RTM.EXE`, `DPMI32VM.OVL`, `INSTALL.EXE`, `SETUP.EXE`, `LOADPATS.EXE`, `*.386`, and the `.BNK` files are installer/DOS-extender/sound-driver material from the floppy installation; they are useful for version identification but are not content inputs for `data.bin`.

### CD version

Not yet available. The source contains code paths/strings for an external `bmp_8\*.bmp` asset directory and `%s%smission.dat`, which is consistent with a CD layout that externalises art/data. We will not guess its hashes. When a CD copy is supplied, add its `FIRSTENC.EXE` hash and file list here, and only then add a detector. Unrecognised versions must produce a specific diagnostic, never a best-effort load.

## Complete file manifest
| File | Bytes | SHA-256 |
|---|---:|---|
| `32RTM.EXE` | 152,108 | `28c6394e1e8295d4fce54c30d16890e173f625ccedbbbbbab9e27cdd7e390a56` |
| `BLD_INFO.CFG` | 590 | `caed868378adab0f1fe1508d577240b3030b36de6825a4a7fa5ae81bfd074034` |
| `CABIN0.BMP` | 51,638 | `99bfe4ca6159427a28d9e24ebb70061ea9eb69c93b262d27cd15d0b11c5f3c0a` |
| `CABIN1.BMP` | 51,638 | `dfdc9ad2efe49001c8d9e63218f2c32791540296359309fe8ef5673ad6c9be58` |
| `CABIN2.BMP` | 51,638 | `64e82260b7e118b480e164bcf3c7926b47449d618a3da8d6fb362cc881055ad0` |
| `CABIN3.BMP` | 51,638 | `2acda06a83958ae501ae5e931987deb79feccdff4a54b99bb8dec919e9263d19` |
| `CABIN4.BMP` | 51,638 | `b8a9a1515dee8d83e1e95c5b87ce31c408d503a6ae0dd65403392bade59066bb` |
| `CABIN5.BMP` | 51,638 | `421ef56f28c4bed69a658c74464a29a4cf16d50cadf0ceb5c2cdc1c1d57e06ae` |
| `DPMI32VM.OVL` | 58,344 | `c65e6051c508bcaccee0f96d43abc01ea96e48cd9fb244a438461ce1d67ca6b3` |
| `DRUM.BNK` | 5,404 | `809994c0e42679e4fc4a1e03a835795be2f09e3a0eba6fc56e957b746e060b9c` |
| `EDPMI.SWP` | 4,194,304 | `a70a16cf620277fb03d619bb5207b4d6bb0ea9b8d4beab7460fc0a89388da4af` |
| `FIRSTENC.EXE` | 2,097,152 | `d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72` |
| `FX/AIROPEN.RAW` | 129,614 | `ca1bd7ba29db3eb7094e7d38e0566c2d4d9ddf01f2569f8c6b581e66daacbfc4` |
| `FX/BIP.RAW` | 16,842 | `faa716acdad6330700ef4a1f02966299fdba985d1eacbc3a70eba17a87b652f3` |
| `FX/BONG.RAW` | 23,612 | `3bbd2ec4bcfbbfea02991982493aff4d5cd0534e5906d846afff8920690ba6e5` |
| `FX/BOOM.RAW` | 93,468 | `6dccbcc39b596a460c72de358e5a4750e863a85be971531ac1a5f53055adc038` |
| `FX/CAMERA.RAW` | 11,544 | `2938bb2e836b24c5418f25be68df0d064da91f5fb705c2fb4c0e61d36d06fe8d` |
| `FX/CHIME.RAW` | 39,276 | `660ac4b16eabb6af6590db4693cd05c6e5196d900a9e22e14071cf8bad14671f` |
| `FX/CLUNKIT.RAW` | 3,470 | `d0e94f44e6c4377196b03f298072dfa93388259ac03b205a3732df5628c0b316` |
| `FX/DAMAGE.RAW` | 47,834 | `d9c6fe0e3b187d9f04c20573e885fe740ec5960c5c2dabf060780eb6c60f4a8d` |
| `FX/ECM.RAW` | 28,472 | `4a32fda59c5ac13f61c60bbd4d81d99a8db16d99ddd7f4c2b0a62480ff78c7c5` |
| `FX/HYPE.RAW` | 94,140 | `331f7240521cf0e2a99e797940ac57d218a751bc5fcb5500fc42737570c5cf5c` |
| `FX/LASER1.RAW` | 8,354 | `d59fe11c2a40c6f38568e9cb27c1e60c288eff11fbad87b23472ad80b8554471` |
| `FX/LASER2.RAW` | 14,606 | `52ef0af4c20e0cce5e402fdbbab8c08454c950097ab128cf075bbdf05bae5e57` |
| `FX/LASER3.RAW` | 9,198 | `99031a616a7bf5c62c9c7a08b5925437db77b7d57d04522db9163d9eb4ba3a61` |
| `FX/LASER4.RAW` | 9,198 | `76dbaff801bbc8d8f6c8611831a0d6d705170673307268d2e479c0a244742593` |
| `FX/LAUNCH.RAW` | 51,102 | `a6d710ed90375c2d02f612b3159e8a52f9d3aea4a345a75a9ccdb0fc77f6fac6` |
| `FX/MISSEXPL.RAW` | 49,432 | `9d63c23caf45668f8fc4558252319408273a10ebb753be88a07b9d01c70189f3` |
| `FX/NECM.RAW` | 16,632 | `a84a08137d3d61e2a0b6838ac65a16c3d719a1fcf2e6e562aefb5d18cfccd632` |
| `FX/NOWT.RAW` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `FX/ROAR.RAW` | 1,488 | `3f4d66a3b984ddf9ba09e7be0c68d1ef1a6503b748bc30ddf34079b1556d7b3f` |
| `FX/SHIPEXP.RAW` | 27,880 | `574a5b946358c8208b1c5372b302b5215bc5afdff72151113fe6891a50ea99fc` |
| `FX/SIREN.RAW` | 19,232 | `76a7f87eacdfbb15890f18809a824e06adb1541e6c45c14bb725d556b28ce54d` |
| `FX/STATION.RAW` | 16,362 | `47cd3363df3c368374a13b286e0d88522b0b58c566f13ddc4b84ef8ed37126ff` |
| `FX/WIND.RAW` | 56,152 | `026e4c717931a691397feb9b07135eedffb32c341ba28c3291339fabbc90f029` |
| `GAMETEK.1` | 739,463 | `c422886d0f7f091a3865cdf0699802c833fda720acc93a7f884eac5754d5ecf5` |
| `HMIDET.386` | 81,990 | `8d800cd95214f1cd6e98e14bc1ed9608f2da9e50d60614caa4628821ee4e44a1` |
| `HMIDRV.386` | 261,425 | `f95ac0259cc1f0512f51ed9fcf97478c628bbaff58bbc2cba5873a352bad2209` |
| `HMIMDRV.386` | 117,816 | `db8434931aa18ac6f25004bd5fa9cec8ba2d98c13f4dd1da4d3080efa9eba5ee` |
| `HMISET.CFG` | 227 | `e255efb00289422abe311c173f500f54acc44108ca434d19e598674554309bd6` |
| `IMAGE.DAT` | 37 | `fd396a38da89fbeb4548e82c6c6de3fd60012381e47155025d4166d0306d1307` |
| `INSTALL.EXE` | 76,470 | `cc42cccea881f15aa8ecdbf7ef80617bf3bf2e44792dd7be5cc78ff60d7495e3` |
| `LANGRC.RES` | 6,243 | `991adaf8986c341019567e703636df757b7bf54daf40d81d0d3604b5cd96d5e4` |
| `LOADPATS.EXE` | 49,194 | `bb9a4f0f3beb2c2294175d3bbecbcff3846fb72d880b7956245d79ff3172b899` |
| `MELODIC.BNK` | 5,404 | `153b37b7516e2b1cc3008d7958ebb2c9f360353a74cbcf45a7a7d3ea9440236e` |
| `MEMORY.TXT` | 55 | `0cfabfeccda9f3db1dd6472957fced80d3c3fce99770db1af1c519cde08ff2f1` |
| `MISSION.DAT` | 779,951 | `3cf36755c7420f12281ca8a55f11dbc9787bacaeef49edc4584338f77dc94afc` |
| `MUSIC/BABAYAGA.HMP` | 13,141 | `39c29e6359f79d6b0a1f6b0fd1fe037310c703e2c61a58b8979b04f9662c1c48` |
| `MUSIC/BAREM.HMP` | 37,033 | `76dc0d654f22543da1bb3e52a3ff1544e27a57ee457e8d07d17d5f506f66696e` |
| `MUSIC/BLUEDAN.HMP` | 12,723 | `cd5ee6f4be03dfbe33187ba28df8e0dcaf055289fb1314ad270dba99952fe65d` |
| `MUSIC/GGOFKIEV.HMP` | 5,745 | `b26815a53151089562e9dbc2e051a8f2f9a711654678db9c6c1a18da8cf9c566` |
| `MUSIC/HALLKING.HMP` | 21,136 | `5a925591124364b16319bce45527b9f48ce6c69fa9d2d96f2b0eaf197ca2a236` |
| `MUSIC/INTRO.HMP` | 30,084 | `1bc2fc2f987bbd57d0f6deda467e2987822000e50bbae624883726ed1c6e81ba` |
| `MUSIC/JUPALT.HMP` | 29,383 | `005fa7d19724dbaf590a1c4d700cdbbc2bc006594e0ac9b851af3833c616e3e5` |
| `MUSIC/QQATMOSP.HMP` | 27,464 | `519faad9b60d8537d094766bb24168de653dc35b71b3178364d024147743d8ae` |
| `MUSIC/QQDRAMA.HMP` | 33,442 | `2b3dd95769a07d19ad43bd7b3da9cb8792a6285d21693966864b1ad82d447c1c` |
| `MUSIC/QQESCAPE.HMP` | 21,552 | `ae3a293340aae884a33ca0bdd49db578020e2fd290a49f65f056e84916ae9fee` |
| `MUSIC/QQFRONT.HMP` | 16,880 | `28ded21f7f6d3ecbfa7af2515e9ddc1060838cd2e67baef4b8bbad9622b822f9` |
| `MUSIC/QQFRONT2.HMP` | 9,104 | `0b126ed4bb33165ce5af66bda2a3ac020a20e2cf3047a47cc22027da6e3caf74` |
| `MUSIC/QQPARADI.HMP` | 18,684 | `175d475ee369eb98543b4071236ced2f24e37cefcabb974827f48ca746e0dadc` |
| `MUSIC/QQROCK.HMP` | 47,046 | `780b2c6ab14d7252ae04b96453d1140e7c861cc1be2dbb4601ad046510fd2fba` |
| `MUSIC/QQSUSPEN.HMP` | 22,469 | `de9057139f7cd6eeb045ed777bb554f698e414a1fd35b3cb44a16c86a40718b2` |
| `MUSIC/QQTRAVEL.HMP` | 30,353 | `a8e50b92bec2e4ef1997b8b53eacb32656607ef7959dbbcca342f25f4aa5fa9b` |
| `MUSIC/VALKRIES.HMP` | 25,763 | `636e302b7bcdedb8ef87f4da7102a30cc6e8f7a6b52ad541f16bb8e3de215177` |
| `PATCHES.INI` | 9,825 | `2283f9e22d004032a0a896fa21253749e1539f23aeaed69a2c281b10268aa90c` |
| `SETUP.EXE` | 188,279 | `0296420b885ed67a051826f9002250a75243d27bc28a16887f86eec62b16b29c` |
| `SETUP.INI` | 3,353 | `caf4add9e4e96798f57810480832cdb4d71ad153f69a4a0f5bc154043fb5fe62` |
| `SETUP.PIF` | 967 | `744d590a77e49ed010e67271ac672ade19cf43699341d34380642d4306c9b6ac` |
| `TEST.HMP` | 18,643 | `f812a7578d6fb431976b66addbf4cfed2d4b9cb92f0581f5910e2034d8a227a3` |
| `TEST.RAW` | 22,313 | `179f643745c8512e94a6e016f5b45917f501143869ade9409ef0a93e53115597` |
| `dos4gw.exe` | 254,556 | `580f164bd05b438262267a7e03984529b56e3096ee158d5d2b0e6f159520d4c3` |

