use disk_inventory_lib::disks::list_disks;

fn main() {
    println!("Volume information");
    let disks = list_disks();
    for disk in disks {
        println!("Disk: {}", disk.name);
        println!("Mount Point: {}", disk.mount_point);
        //println!("File System: {}", disk.file_system);
        //println!("Kind: {}", disk.kind);
        println!("Removable: {}", disk.is_removable);
        println!("Read Only: {}", disk.is_read_only);
        println!("Total Bytes: {}", disk.total_bytes);
        //println!("Available Bytes: {}", disk.available_bytes);
       // println!("Used Bytes: {}", disk.used_bytes);
        //println!("Used Percent: {:.2}%", disk.used_percent);
        println!("================================");
    }
}