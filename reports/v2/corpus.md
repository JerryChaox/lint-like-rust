# V2 Museon 语料验收

变异是人为注入的测试错误，不是 Museon 原有 bug。verified 仅限报告列出的义务、已建模正常/异常路径及报告假设；不是整个项目安全证明。

| case | variant | actual | rules | gaps | target met |
|---|---|---|---|---:|---|
| read_file | original | verified_within_scope |  | 0 | True |
| read_file | mutated_error | violation | LIFE001 | 0 | True |
| read_file | repaired | verified_within_scope |  | 0 | True |
| read_file | cross_function_error | violation | LIFE001 | 0 | True |
| read_file | cross_function_repaired | verified_within_scope |  | 0 | True |
| path_sha256 | original | unverified |  | 5 | False |
| path_sha256 | mutated_error | unverified |  | 6 | False |
| path_sha256 | repaired | unverified |  | 5 | False |
| append_brain_log | original | unverified |  | 10 | False |
| append_brain_log | mutated_error | unverified |  | 11 | False |
| append_brain_log | repaired | unverified |  | 10 | False |
