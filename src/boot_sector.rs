use core::cmp;
use io;
use io::prelude::*;
use io::{Error, ErrorKind};

use byteorder::LittleEndian;
use byteorder_ext::{ReadBytesExt, WriteBytesExt};

use dir_entry::DIR_ENTRY_SIZE;
use fs::{FatType, FsStatusFlags, FormatVolumeOptions};
use table::RESERVED_FAT_ENTRIES;

const KB: u64 = 1024;
const MB: u64 = KB * 1024;
const GB: u64 = MB * 1024;

#[derive(Default, Debug, Clone)]
pub(crate) struct BiosParameterBlock {
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
    pub(crate) reserved_sectors: u16,
    pub(crate) fats: u8,
    pub(crate) root_entries: u16,
    pub(crate) total_sectors_16: u16,
    pub(crate) media: u8,
    pub(crate) sectors_per_fat_16: u16,
    pub(crate) sectors_per_track: u16,
    pub(crate) heads: u16,
    pub(crate) hidden_sectors: u32,
    pub(crate) total_sectors_32: u32,

    // Extended BIOS Parameter Block
    pub(crate) sectors_per_fat_32: u32,
    pub(crate) extended_flags: u16,
    pub(crate) fs_version: u16,
    pub(crate) root_dir_first_cluster: u32,
    pub(crate) fs_info_sector: u16,
    pub(crate) backup_boot_sector: u16,
    pub(crate) reserved_0: [u8; 12],
    pub(crate) drive_num: u8,
    pub(crate) reserved_1: u8,
    pub(crate) ext_sig: u8,
    pub(crate) volume_id: u32,
    pub(crate) volume_label: [u8; 11],
    pub(crate) fs_type_label: [u8; 8],
}

impl BiosParameterBlock {
    fn deserialize<T: Read>(rdr: &mut T) -> io::Result<BiosParameterBlock> {
        let mut bpb: BiosParameterBlock = Default::default();
        bpb.bytes_per_sector = rdr.read_u16::<LittleEndian>()?;
        bpb.sectors_per_cluster = rdr.read_u8()?;
        bpb.reserved_sectors = rdr.read_u16::<LittleEndian>()?;
        bpb.fats = rdr.read_u8()?;
        bpb.root_entries = rdr.read_u16::<LittleEndian>()?;
        bpb.total_sectors_16 = rdr.read_u16::<LittleEndian>()?;
        bpb.media = rdr.read_u8()?;
        bpb.sectors_per_fat_16 = rdr.read_u16::<LittleEndian>()?;
        bpb.sectors_per_track = rdr.read_u16::<LittleEndian>()?;
        bpb.heads = rdr.read_u16::<LittleEndian>()?;
        bpb.hidden_sectors = rdr.read_u32::<LittleEndian>()?;
        bpb.total_sectors_32 = rdr.read_u32::<LittleEndian>()?;

        if bpb.is_fat32() {
            bpb.sectors_per_fat_32 = rdr.read_u32::<LittleEndian>()?;
            bpb.extended_flags = rdr.read_u16::<LittleEndian>()?;
            bpb.fs_version = rdr.read_u16::<LittleEndian>()?;
            bpb.root_dir_first_cluster = rdr.read_u32::<LittleEndian>()?;
            bpb.fs_info_sector = rdr.read_u16::<LittleEndian>()?;
            bpb.backup_boot_sector = rdr.read_u16::<LittleEndian>()?;
            rdr.read_exact(&mut bpb.reserved_0)?;
            bpb.drive_num = rdr.read_u8()?;
            bpb.reserved_1 = rdr.read_u8()?;
            bpb.ext_sig = rdr.read_u8()?; // 0x29
            bpb.volume_id = rdr.read_u32::<LittleEndian>()?;
            rdr.read_exact(&mut bpb.volume_label)?;
            rdr.read_exact(&mut bpb.fs_type_label)?;
        } else {
            bpb.drive_num = rdr.read_u8()?;
            bpb.reserved_1 = rdr.read_u8()?;
            bpb.ext_sig = rdr.read_u8()?; // 0x29
            bpb.volume_id = rdr.read_u32::<LittleEndian>()?;
            rdr.read_exact(&mut bpb.volume_label)?;
            rdr.read_exact(&mut bpb.fs_type_label)?;
        }

        // when the extended boot signature is anything other than 0x29, the fields are invalid
        if bpb.ext_sig != 0x29 {
            // fields after ext_sig are not used - clean them
            bpb.volume_id = 0;
            bpb.volume_label = [0; 11];
            bpb.fs_type_label = [0; 8];
        }

        Ok(bpb)
    }

    fn serialize<T: Write>(&self, mut wrt: T) -> io::Result<()> {
        wrt.write_u16::<LittleEndian>(self.bytes_per_sector)?;
        wrt.write_u8(self.sectors_per_cluster)?;
        wrt.write_u16::<LittleEndian>(self.reserved_sectors)?;
        wrt.write_u8(self.fats)?;
        wrt.write_u16::<LittleEndian>(self.root_entries)?;
        wrt.write_u16::<LittleEndian>(self.total_sectors_16)?;
        wrt.write_u8(self.media)?;
        wrt.write_u16::<LittleEndian>(self.sectors_per_fat_16)?;
        wrt.write_u16::<LittleEndian>(self.sectors_per_track)?;
        wrt.write_u16::<LittleEndian>(self.heads)?;
        wrt.write_u32::<LittleEndian>(self.hidden_sectors)?;
        wrt.write_u32::<LittleEndian>(self.total_sectors_32)?;

        if self.is_fat32() {
            wrt.write_u32::<LittleEndian>(self.sectors_per_fat_32)?;
            wrt.write_u16::<LittleEndian>(self.extended_flags)?;
            wrt.write_u16::<LittleEndian>(self.fs_version)?;
            wrt.write_u32::<LittleEndian>(self.root_dir_first_cluster)?;
            wrt.write_u16::<LittleEndian>(self.fs_info_sector)?;
            wrt.write_u16::<LittleEndian>(self.backup_boot_sector)?;
            wrt.write_all(&self.reserved_0)?;
            wrt.write_u8(self.drive_num)?;
            wrt.write_u8(self.reserved_1)?;
            wrt.write_u8(self.ext_sig)?; // 0x29
            wrt.write_u32::<LittleEndian>(self.volume_id)?;
            wrt.write_all(&self.volume_label)?;
            wrt.write_all(&self.fs_type_label)?;
        } else {
            wrt.write_u8(self.drive_num)?;
            wrt.write_u8(self.reserved_1)?;
            wrt.write_u8(self.ext_sig)?; // 0x29
            wrt.write_u32::<LittleEndian>(self.volume_id)?;
            wrt.write_all(&self.volume_label)?;
            wrt.write_all(&self.fs_type_label)?;
        }
        Ok(())
    }

    fn validate(&self) -> io::Result<()> {
        // sanity checks
        if self.bytes_per_sector.count_ones() != 1 {
            return Err(Error::new(
                ErrorKind::Other,
                "invalid bytes_per_sector value in BPB (not power of two)",
            ));
        } else if self.bytes_per_sector < 512 {
            return Err(Error::new(ErrorKind::Other, "invalid bytes_per_sector value in BPB (value < 512)"));
        } else if self.bytes_per_sector > 4096 {
            return Err(Error::new(ErrorKind::Other, "invalid bytes_per_sector value in BPB (value > 4096)"));
        }

        if self.sectors_per_cluster.count_ones() != 1 {
            return Err(Error::new(
                ErrorKind::Other,
                "invalid sectors_per_cluster value in BPB (not power of two)",
            ));
        } else if self.sectors_per_cluster < 1 {
            return Err(Error::new(ErrorKind::Other, "invalid sectors_per_cluster value in BPB (value < 1)"));
        } else if self.sectors_per_cluster > 128 {
            return Err(Error::new(
                ErrorKind::Other,
                "invalid sectors_per_cluster value in BPB (value > 128)",
            ));
        }

        // bytes per sector is u16, sectors per cluster is u8, so guaranteed no overflow in multiplication
        let bytes_per_cluster = self.bytes_per_sector as u32 * self.sectors_per_cluster as u32;
        let maximum_compatibility_bytes_per_cluster: u32 = 32 * 1024;

        if bytes_per_cluster > maximum_compatibility_bytes_per_cluster {
            // 32k is the largest value to maintain greatest compatibility
            // Many implementations appear to support 64k per cluster, and some may support 128k or larger
            // However, >32k is not as thoroughly tested...
            warn!("fs compatibility: bytes_per_cluster value '{}' in BPB exceeds '{}', and thus may be incompatible with some implementations",
                bytes_per_cluster, maximum_compatibility_bytes_per_cluster);
        }

        let is_fat32 = self.is_fat32();
        if self.reserved_sectors < 1 {
            return Err(Error::new(ErrorKind::Other, "invalid reserved_sectors value in BPB"));
        } else if !is_fat32 && self.reserved_sectors != 1 {
            // Microsoft document indicates fat12 and fat16 code exists that presume this value is 1
            warn!(
                "fs compatibility: reserved_sectors value '{}' in BPB is not '1', and thus is incompatible with some implementations",
                self.reserved_sectors
            );
        }

        if self.fats == 0 {
            return Err(Error::new(ErrorKind::Other, "invalid fats value in BPB"));
        } else if self.fats > 2 {
            // Microsoft document indicates that few implementations support any values other than 1 or 2
            warn!(
                "fs compatibility: numbers of FATs '{}' in BPB is greater than '2', and thus is incompatible with some implementations",
                self.fats
            );
        }

        if is_fat32 && self.root_entries != 0 {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid root_entries value in BPB (should be zero for FAT32)",
            ));
        }

        if is_fat32 && self.total_sectors_16 != 0 {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid total_sectors_16 value in BPB (should be zero for FAT32)",
            ));
        }

        if (self.total_sectors_16 == 0) == (self.total_sectors_32 == 0) {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid BPB (total_sectors_16 or total_sectors_32 should be non-zero)",
            ));
        }

        if is_fat32 && self.sectors_per_fat_32 == 0 {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid sectors_per_fat_32 value in BPB (should be non-zero for FAT32)",
            ));
        }

        if self.fs_version != 0 {
            return Err(Error::new(ErrorKind::Other, "Unknown FS version"));
        }

        if self.total_sectors() <= self.first_data_sector() {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid BPB (total_sectors field value is too small)",
            ));
        }

        let total_clusters = self.total_clusters();
        let fat_type = FatType::from_clusters(total_clusters);
        if is_fat32 != (fat_type == FatType::Fat32) {
            return Err(Error::new(
                ErrorKind::Other,
                "Invalid BPB (result of FAT32 determination from total number of clusters and sectors_per_fat_16 field differs)",
            ));
        }

        let bits_per_fat_entry = fat_type.bits_per_fat_entry();
        let total_fat_entries = self.sectors_per_fat() * self.bytes_per_sector as u32 * 8 / bits_per_fat_entry as u32;
        if total_fat_entries - RESERVED_FAT_ENTRIES < total_clusters {
            warn!("FAT is too small to compared to total number of clusters");
        }

        Ok(())
    }

    pub(crate) fn mirroring_enabled(&self) -> bool {
        self.extended_flags & 0x80 == 0
    }

    pub(crate) fn active_fat(&self) -> u16 {
        // The zero-based number of the active FAT is only valid if mirroring is disabled.
        if self.mirroring_enabled() {
            0
        } else {
            self.extended_flags & 0x0F
        }
    }

    pub(crate) fn status_flags(&self) -> FsStatusFlags {
        FsStatusFlags::decode(self.reserved_1)
    }

    pub(crate) fn is_fat32(&self) -> bool {
        // because this field must be zero on FAT32, and
        // because it must be non-zero on FAT12/FAT16,
        // this provides a simple way to detect FAT32
        self.sectors_per_fat_16 == 0
    }

    pub(crate) fn sectors_per_fat(&self) -> u32 {
        if self.is_fat32() {
            self.sectors_per_fat_32
        } else {
            self.sectors_per_fat_16 as u32
        }
    }

    pub(crate) fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 == 0 {
            self.total_sectors_32
        } else {
            self.total_sectors_16 as u32
        }
    }

    pub(crate) fn reserved_sectors(&self) -> u32 {
        self.reserved_sectors as u32
    }

    pub(crate) fn root_dir_sectors(&self) -> u32 {
        let root_dir_bytes = self.root_entries as u32 * DIR_ENTRY_SIZE as u32;
        (root_dir_bytes + self.bytes_per_sector as u32 - 1) / self.bytes_per_sector as u32
    }

    pub(crate) fn sectors_per_all_fats(&self) -> u32 {
        self.fats as u32 * self.sectors_per_fat()
    }

    pub(crate) fn first_data_sector(&self) -> u32 {
        let root_dir_sectors = self.root_dir_sectors();
        let fat_sectors = self.sectors_per_all_fats();
        self.reserved_sectors() + fat_sectors + root_dir_sectors
    }

    pub(crate) fn total_clusters(&self) -> u32 {
        let total_sectors = self.total_sectors();
        let first_data_sector = self.first_data_sector();
        let data_sectors = total_sectors - first_data_sector;
        data_sectors / self.sectors_per_cluster as u32
    }

    pub(crate) fn bytes_from_sectors(&self, sectors: u32) -> u64 {
        // Note: total number of sectors is a 32 bit number so offsets have to be 64 bit
        (sectors as u64) * self.bytes_per_sector as u64
    }

    pub(crate) fn sectors_from_clusters(&self, clusters: u32) -> u32 {
        // Note: total number of sectors is a 32 bit number so it should not overflow
        clusters * (self.sectors_per_cluster as u32)
    }

    pub(crate) fn cluster_size(&self) -> u32 {
        self.sectors_per_cluster as u32 * self.bytes_per_sector as u32
    }

    pub(crate) fn clusters_from_bytes(&self, bytes: u64) -> u32 {
        let cluster_size = self.cluster_size() as i64;
        ((bytes as i64 + cluster_size - 1) / cluster_size) as u32
    }

    pub(crate) fn fs_info_sector(&self) -> u32 {
        self.fs_info_sector as u32
    }

    pub(crate) fn backup_boot_sector(&self) -> u32 {
        self.backup_boot_sector as u32
    }
}

pub(crate) struct BootSector {
    bootjmp: [u8; 3],
    oem_name: [u8; 8],
    pub(crate) bpb: BiosParameterBlock,
    boot_code: [u8; 448],
    boot_sig: [u8; 2],
}

impl BootSector {
    pub(crate) fn deserialize<T: Read>(rdr: &mut T) -> io::Result<BootSector> {
        let mut boot: BootSector = Default::default();
        rdr.read_exact(&mut boot.bootjmp)?;
        rdr.read_exact(&mut boot.oem_name)?;
        boot.bpb = BiosParameterBlock::deserialize(rdr)?;

        if boot.bpb.is_fat32() {
            rdr.read_exact(&mut boot.boot_code[0..420])?;
        } else {
            rdr.read_exact(&mut boot.boot_code[0..448])?;
        }
        rdr.read_exact(&mut boot.boot_sig)?;
        Ok(boot)
    }

    pub(crate) fn serialize<T: Write>(&self, mut wrt: T) -> io::Result<()> {
        wrt.write_all(&self.bootjmp)?;
        wrt.write_all(&self.oem_name)?;
        self.bpb.serialize(&mut wrt)?;

        if self.bpb.is_fat32() {
            wrt.write_all(&self.boot_code[0..420])?;
        } else {
            wrt.write_all(&self.boot_code[0..448])?;
        }
        wrt.write_all(&self.boot_sig)?;
        Ok(())
    }

    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.boot_sig != [0x55, 0xAA] {
            return Err(Error::new(ErrorKind::Other, "Invalid boot sector signature"));
        }
        if self.bootjmp[0] != 0xEB && self.bootjmp[0] != 0xE9 {
            warn!("Unknown opcode {:x} in bootjmp boot sector field", self.bootjmp[0]);
        }
        self.bpb.validate()?;
        Ok(())
    }
}

impl Default for BootSector {
    fn default() -> BootSector {
        BootSector {
            bootjmp: Default::default(),
            oem_name: Default::default(),
            bpb: Default::default(),
            boot_code: [0; 448],
            boot_sig: Default::default(),
        }
    }
}

pub(crate) fn determine_fat_type(total_bytes: u64) -> FatType {
    if total_bytes < 4 * MB {
        FatType::Fat12
    } else if total_bytes < 512 * MB {
        FatType::Fat16
    } else {
        FatType::Fat32
    }
}

fn determine_bytes_per_cluster(total_bytes: u64, fat_type: FatType, bytes_per_sector: u16) -> u32 {
    let bytes_per_cluster = match fat_type {
        FatType::Fat12 => (total_bytes.next_power_of_two() / MB * 512) as u32,
        FatType::Fat16 => {
            if total_bytes <= 16 * MB {
                1 * KB as u32
            } else if total_bytes <= 128 * MB {
                2 * KB as u32
            } else {
                (total_bytes.next_power_of_two() / (64 * MB) * KB) as u32
            }
        },
        FatType::Fat32 => {
            if total_bytes <= 260 * MB {
                512
            } else if total_bytes <= 8 * GB {
                4 * KB as u32
            } else {
                (total_bytes.next_power_of_two() / (2 * GB) * KB) as u32
            }
        },
    };
    const MAX_CLUSTER_SIZE: u32 = 32 * KB as u32;
    debug_assert!(bytes_per_cluster.is_power_of_two());
    cmp::min(cmp::max(bytes_per_cluster, bytes_per_sector as u32), MAX_CLUSTER_SIZE)
}

fn determine_sectors_per_fat(total_sectors: u32, reserved_sectors: u16, fats: u8, root_dir_sectors: u32,
        sectors_per_cluster: u8, fat_type: FatType) -> u32 {

    // TODO: check if this calculation is always correct (especially for FAT12)
    let tmp_val1 = total_sectors - (reserved_sectors as u32 + root_dir_sectors as u32);
    let mut tmp_val2 = (256 * sectors_per_cluster as u32) + fats as u32;
    if fat_type == FatType::Fat32 {
        tmp_val2 = tmp_val2 / 2;
    } else if fat_type == FatType::Fat12 {
        tmp_val2 = tmp_val2 / 3 * 4
    }
    (tmp_val1 + (tmp_val2 - 1)) / tmp_val2
}

fn format_bpb(options: &FormatVolumeOptions) -> io::Result<(BiosParameterBlock, FatType)> {
    // TODO: maybe total_sectors could be optional?
    let bytes_per_sector = options.bytes_per_sector;
    let total_sectors = options.total_sectors;
    let total_bytes = total_sectors as u64 * bytes_per_sector as u64;
    let fat_type = options.fat_type.unwrap_or_else(|| determine_fat_type(total_bytes));
    let bytes_per_cluster = options.bytes_per_cluster
        .unwrap_or_else(|| determine_bytes_per_cluster(total_bytes, fat_type, bytes_per_sector));
    let sectors_per_cluster = (bytes_per_cluster / bytes_per_sector as u32) as u8;

    // Note: most of implementations use 32 reserved sectors for FAT32 but it's wasting of space
    // We use 4 because there are two boot sectors and one FS Info sector (1 sector remains unused)
    let reserved_sectors: u16 = if fat_type == FatType::Fat32 { 4 } else { 1 };

    let fats = 2u8;
    let is_fat32 = fat_type == FatType::Fat32;
    let root_entries = if is_fat32 { 0 } else { options.root_entries.unwrap_or(512) };
    let root_dir_bytes = root_entries as u32 * DIR_ENTRY_SIZE as u32;
    let root_dir_sectors = (root_dir_bytes + bytes_per_sector as u32 - 1) / bytes_per_sector as u32;

    // Check if volume has enough space to accomodate reserved sectors, FAT, root directory and some data space
    // Having less than 8 sectors for FAT and data would make a little sense
    if total_sectors <= reserved_sectors as u32 + root_dir_sectors as u32 + 8 {
        return Err(Error::new(ErrorKind::Other, "Volume is too small",));
    }

    // calculate File Allocation Table size
    let sectors_per_fat = determine_sectors_per_fat(total_sectors, reserved_sectors, fats, root_dir_sectors,
        sectors_per_cluster, fat_type);

    // drive_num should be 0 for floppy disks and 0x80 for hard disks - determine it using FAT type
    let drive_num = options.drive_num.unwrap_or_else(|| if fat_type == FatType::Fat12 { 0 } else { 0x80 });

    // reserved_0 is always zero
    let reserved_0 = [0u8; 12];

    // setup volume label
    let mut volume_label = [0u8; 11];
    if let Some(volume_label_from_opts) = options.volume_label {
        volume_label.copy_from_slice(&volume_label_from_opts);
    } else {
        volume_label.copy_from_slice(b"NO NAME    ");
    }

    // setup fs_type_label field
    let mut fs_type_label = [0u8; 8];
    let fs_type_label_str = match fat_type {
        FatType::Fat12 => b"FAT12   ",
        FatType::Fat16 => b"FAT16   ",
        FatType::Fat32 => b"FAT32   ",
    };
    fs_type_label.copy_from_slice(fs_type_label_str);

    // create Bios Parameter Block struct
    let bpb = BiosParameterBlock {
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        fats,
        root_entries,
        total_sectors_16: if total_sectors < 0x10000 { total_sectors as u16 } else { 0 },
        media: options.media.unwrap_or(0xF8),
        sectors_per_fat_16: if is_fat32 { 0 } else { sectors_per_fat as u16 },
        sectors_per_track: options.sectors_per_track.unwrap_or(0x20),
        heads: options.heads.unwrap_or(0x40),
        hidden_sectors: 0,
        total_sectors_32: if total_sectors >= 0x10000 { total_sectors } else { 0 },
        // FAT32 fields start
        sectors_per_fat_32: if is_fat32 { sectors_per_fat } else { 0 },
        extended_flags: 0, // mirroring enabled
        fs_version: 0,
        root_dir_first_cluster: if is_fat32 { 2 } else { 0 },
        fs_info_sector: if is_fat32 { 1 } else { 0 },
        backup_boot_sector: if is_fat32 { 6 } else { 0 },
        reserved_0,
        // FAT32 fields end
        drive_num,
        reserved_1: 0,
        ext_sig: 0x29,
        volume_id: options.volume_id.unwrap_or(0x12345678),
        volume_label,
        fs_type_label,
    };

    // Check if number of clusters is proper for used FAT type
    if FatType::from_clusters(bpb.total_clusters()) != fat_type {
        return Err(Error::new(ErrorKind::Other, "Total number of clusters and FAT type does not match. Try other volume size"));
    }

    Ok((bpb, fat_type))
}

pub(crate) fn format_boot_sector(options: &FormatVolumeOptions) -> io::Result<(BootSector, FatType)> {
    let mut boot: BootSector = Default::default();
    let (bpb, fat_type) = format_bpb(options)?;
    boot.bpb = bpb;
    boot.oem_name.copy_from_slice(b"MSWIN4.1");
    // Boot code copied from FAT32 boot sector initialized by mkfs.fat
    boot.bootjmp = [0xEB, 0x58, 0x90];
    let boot_code: [u8; 129] = [
        0x0E, 0x1F, 0xBE, 0x77, 0x7C, 0xAC, 0x22, 0xC0, 0x74, 0x0B, 0x56, 0xB4, 0x0E, 0xBB, 0x07, 0x00,
        0xCD, 0x10, 0x5E, 0xEB, 0xF0, 0x32, 0xE4, 0xCD, 0x16, 0xCD, 0x19, 0xEB, 0xFE, 0x54, 0x68, 0x69,
        0x73, 0x20, 0x69, 0x73, 0x20, 0x6E, 0x6F, 0x74, 0x20, 0x61, 0x20, 0x62, 0x6F, 0x6F, 0x74, 0x61,
        0x62, 0x6C, 0x65, 0x20, 0x64, 0x69, 0x73, 0x6B, 0x2E, 0x20, 0x20, 0x50, 0x6C, 0x65, 0x61, 0x73,
        0x65, 0x20, 0x69, 0x6E, 0x73, 0x65, 0x72, 0x74, 0x20, 0x61, 0x20, 0x62, 0x6F, 0x6F, 0x74, 0x61,
        0x62, 0x6C, 0x65, 0x20, 0x66, 0x6C, 0x6F, 0x70, 0x70, 0x79, 0x20, 0x61, 0x6E, 0x64, 0x0D, 0x0A,
        0x70, 0x72, 0x65, 0x73, 0x73, 0x20, 0x61, 0x6E, 0x79, 0x20, 0x6B, 0x65, 0x79, 0x20, 0x74, 0x6F,
        0x20, 0x74, 0x72, 0x79, 0x20, 0x61, 0x67, 0x61, 0x69, 0x6E, 0x20, 0x2E, 0x2E, 0x2E, 0x20, 0x0D,
        0x0A];
    boot.boot_code[..boot_code.len()].copy_from_slice(&boot_code);
    boot.boot_sig = [0x55, 0xAA];

    // fix offsets in bootjmp and boot code for non-FAT32 filesystems (bootcode is on a different offset)
    if fat_type != FatType::Fat32 {
        // offset of boot code
        let boot_code_offset = 0x36 + 8;
        boot.bootjmp[1] = (boot_code_offset - 2) as u8;
        // offset of message
        const MESSAGE_OFFSET: u32 = 29;
        let message_offset_in_sector = boot_code_offset + MESSAGE_OFFSET + 0x7c00;
        boot.boot_code[3] = (message_offset_in_sector & 0xff) as u8;
        boot.boot_code[4] = (message_offset_in_sector >> 8) as u8;
    }

    Ok((boot, fat_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_fat_type() {
        assert_eq!(determine_fat_type(3 * MB), FatType::Fat12);
        assert_eq!(determine_fat_type(4 * MB), FatType::Fat16);
        assert_eq!(determine_fat_type(511 * MB), FatType::Fat16);
        assert_eq!(determine_fat_type(512 * MB), FatType::Fat32);
    }

    #[test]
    fn test_determine_bytes_per_cluster_fat12() {
        assert_eq!(determine_bytes_per_cluster(1 * MB + 0, FatType::Fat12, 512), 512);
        assert_eq!(determine_bytes_per_cluster(1 * MB + 1, FatType::Fat12, 512), 1024);
        assert_eq!(determine_bytes_per_cluster(1 * MB, FatType::Fat12, 4096), 4096);
    }

    #[test]
    fn test_determine_bytes_per_cluster_fat16() {
        assert_eq!(determine_bytes_per_cluster(1 * MB, FatType::Fat16, 512), 1 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(1 * MB, FatType::Fat16, 4 * KB as u16), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(16 * MB + 0, FatType::Fat16, 512), 1 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(16 * MB + 1, FatType::Fat16, 512), 2 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(128 * MB + 0, FatType::Fat16, 512), 2 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(128 * MB + 1, FatType::Fat16, 512), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(256 * MB + 0, FatType::Fat16, 512), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(256 * MB + 1, FatType::Fat16, 512), 8 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(512 * MB + 0, FatType::Fat16, 512), 8 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(512 * MB + 1, FatType::Fat16, 512), 16 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(1024 * MB + 0, FatType::Fat16, 512), 16 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(1024 * MB + 1, FatType::Fat16, 512), 32 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(99999 * MB, FatType::Fat16, 512), 32 * KB as u32);
    }

    #[test]
    fn test_determine_bytes_per_cluster_fat32() {
        assert_eq!(determine_bytes_per_cluster(260 * MB as u64, FatType::Fat32, 512), 512);
        assert_eq!(determine_bytes_per_cluster(260 * MB as u64, FatType::Fat32, 4 * KB as u16), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(260 * MB as u64 + 1, FatType::Fat32, 512), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(8 * GB as u64, FatType::Fat32, 512), 4 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(8 * GB as u64 + 1, FatType::Fat32, 512), 8 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(16 * GB as u64 + 0, FatType::Fat32, 512), 8 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(16 * GB as u64 + 1, FatType::Fat32, 512), 16 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(32 * GB as u64, FatType::Fat32, 512), 16 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(32 * GB as u64 + 1, FatType::Fat32, 512), 32 * KB as u32);
        assert_eq!(determine_bytes_per_cluster(999 * GB as u64, FatType::Fat32, 512), 32 * KB as u32);
    }

    #[test]
    fn test_determine_sectors_per_fat() {
        assert_eq!(determine_sectors_per_fat(1 * MB as u32 / 512, 1, 2, 32, 1, FatType::Fat12), 6);
    }
}
