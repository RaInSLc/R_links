#include <iostream>
#include <string>
#include <cassert>
#include <vector>

// 声明测试函数（直接搬运 pkg_logic.cpp 中的逻辑用于单元测试）
int TestParseMinorVersion(const std::wstring& version) {
    size_t first_dot = version.find(L'.');
    if (first_dot == std::wstring::npos) return -1;
    size_t second_dot = version.find(L'.', first_dot + 1);
    std::wstring minor_str;
    if (second_dot != std::wstring::npos) {
        minor_str = version.substr(first_dot + 1, second_dot - first_dot - 1);
    } else {
        minor_str = version.substr(first_dot + 1);
    }
    try {
        return std::stoi(minor_str);
    } catch (...) {
        return -1;
    }
}

int TestInferBiocVersion(int pkgMinorVersion) {
    if (pkgMinorVersion >= 34) {
        return 3 + (pkgMinorVersion - 34) / 2;
    }
    return -1;
}

std::wstring TestGenerateBiocRef(int biocVersionMajor, int biocVersionMinor) {
    return L"RELEASE_" + std::to_wstring(biocVersionMajor) + L"_" + std::to_wstring(biocVersionMinor);
}

// 统一的 HTML TD 提取安全测试逻辑
std::string TestSafeExtractTDValue(const std::string& html, const std::string& pattern, size_t startPos = 0) {
    size_t pos = html.find(pattern, startPos);
    if (pos == std::string::npos) return "";
    size_t next_td = html.find("<td>", pos + pattern.length());
    if (next_td == std::string::npos) return "";
    size_t end_td = html.find("</td>", next_td + 4);
    if (end_td == std::string::npos || end_td <= next_td + 4) return "";
    std::string val = html.substr(next_td + 4, end_td - (next_td + 4));
    val.erase(0, val.find_first_not_of(" \t\r\n"));
    size_t last_not_space = val.find_last_not_of(" \t\r\n");
    if (last_not_space != std::string::npos) {
        val.erase(last_not_space + 1);
    } else {
        val.clear();
    }
    return val;
}

// 统一的中文版本判断测试
bool TestHasChinese(const std::wstring& v) {
    bool hasChinese = false;
    for (wchar_t ch : v) {
        if (ch >= 0x4E00 && ch <= 0x9FA5) {
            hasChinese = true;
            break;
        }
    }
    return hasChinese;
}

bool TestIsVersionCompatible(const std::wstring& histVer, const std::wstring& userVer) {
    if (histVer == userVer) return true;
    size_t user_first_dot = userVer.find(L'.');
    if (user_first_dot != std::wstring::npos) {
        size_t user_second_dot = userVer.find(L'.', user_first_dot + 1);
        if (user_second_dot == std::wstring::npos) {
            if (histVer.length() > userVer.length() && histVer.compare(0, userVer.length(), userVer) == 0) {
                if (histVer[userVer.length()] == L'.') {
                    return true;
                }
            }
        }
    }
    return false;
}

int main() {
    std::cout << "[*] Starting offline C++ logic tests..." << std::endl;

    // 1. 验证版本提取 ParseMinorVersion
    assert(TestParseMinorVersion(L"1.50.5") == 50);
    assert(TestParseMinorVersion(L"1.52.0") == 52);
    assert(TestParseMinorVersion(L"2.0") == 0);
    assert(TestParseMinorVersion(L"invalid") == -1);
    std::cout << "[+] ParseMinorVersion test passed." << std::endl;

    // 2. 验证推导 InferBiocVersion
    assert(TestInferBiocVersion(50) == 11); // 3 + (50-34)/2 = 3 + 8 = 11 -> 对应的其实是 3.18 (biocVal=11-3=8)
    assert(TestInferBiocVersion(52) == 12); // 3 + (52-34)/2 = 3 + 9 = 12 -> 对应 3.19
    assert(TestInferBiocVersion(34) == 3);  // 3 + 0 = 3 -> 对应 3.0
    assert(TestInferBiocVersion(30) == -1); // 无法推断
    std::cout << "[+] InferBiocVersion test passed." << std::endl;

    // 3. 验证 Ref 生成 GenerateBiocRef
    assert(TestGenerateBiocRef(3, 18) == L"RELEASE_3_18");
    assert(TestGenerateBiocRef(3, 21) == L"RELEASE_3_21");
    std::cout << "[+] GenerateBiocRef test passed." << std::endl;

    // 4. 验证 TD 字段提取防御性安全逻辑 (防止 substr 越界)
    std::string mockHtmlGood = "<tr><td>Version</td><td>1.50.5</td></tr>";
    assert(TestSafeExtractTDValue(mockHtmlGood, "<td>Version") == "1.50.5");

    std::string mockHtmlBad1 = "<tr><td>Version</td><td></td></tr>"; // 空内容
    assert(TestSafeExtractTDValue(mockHtmlBad1, "<td>Version") == "");

    std::string mockHtmlBad2 = "<tr><td>Version</td><td>"; // 截断
    assert(TestSafeExtractTDValue(mockHtmlBad2, "<td>Version") == "");

    std::string mockHtmlBad3 = "<tr><td>Version</td>"; // 完全无 td
    assert(TestSafeExtractTDValue(mockHtmlBad3, "<td>Version") == "");
    std::cout << "[+] SafeExtractTDValue (boundary protection) test passed." << std::endl;

    // 5. 验证中文版本号测试
    assert(TestHasChinese(L"1.50版") == true);
    assert(TestHasChinese(L"1.50-5") == false); // 修复了 find_first_of 的 Bug，以前 1.50-5 会被误判
    assert(TestHasChinese(L"1.50-alpha") == false);
    std::cout << "[+] Chinese version detection range check test passed." << std::endl;

    // 6. 验证兼容性版本匹配 (主次版本相同)
    assert(TestIsVersionCompatible(L"1.50.5", L"1.50") == true);
    assert(TestIsVersionCompatible(L"1.50.5", L"1.50.5") == true);
    assert(TestIsVersionCompatible(L"1.50.5", L"1.5") == false);
    assert(TestIsVersionCompatible(L"1.5.2", L"1.5") == true);
    assert(TestIsVersionCompatible(L"1.50.5", L"1.50.2") == false);
    std::cout << "[+] IsVersionCompatible matching test passed." << std::endl;

    std::cout << "[***] ALL LOGIC TESTS PASSED SUCCESSFULLY! [***]" << std::endl;
    return 0;
}
