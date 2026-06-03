#include <iostream>
#include <string>
#include <cassert>

// 提取包主版本号
int ParseMajorVersion(const std::wstring& version) {
    size_t first_dot = version.find(L'.');
    if (first_dot == std::wstring::npos) return -1;
    try {
        return std::stoi(version.substr(0, first_dot));
    } catch (...) {
        return -1;
    }
}

// 提取包次版本号
int ParseMinorVersion(const std::wstring& version) {
    size_t first_dot = version.find(L'.');
    if (first_dot == std::wstring::npos) return -1;
    size_t second_dot = version.find(L'.', first_dot + 1);
    std::wstring minor_str = (second_dot == std::wstring::npos) 
        ? version.substr(first_dot + 1) 
        : version.substr(first_dot + 1, second_dot - first_dot - 1);
    try {
        return std::stoi(minor_str);
    } catch (...) {
        return -1;
    }
}

// 联合推断 Bioconductor 版本号
int InferBiocVersion(int pkgMajorVersion, int pkgMinorVersion) {
    if (pkgMajorVersion == 1) {
        if (pkgMinorVersion >= 50 && pkgMinorVersion % 2 == 0) {
            return (pkgMinorVersion - 50) / 2 + 18;
        } else if (pkgMinorVersion >= 34 && pkgMinorVersion < 50 && pkgMinorVersion % 2 == 0) {
            return (pkgMinorVersion - 34) / 2;
        }
    } else if (pkgMajorVersion == 2) {
        if (pkgMinorVersion >= 0 && pkgMinorVersion % 2 == 0) {
            return pkgMinorVersion / 2 + 21;
        }
    }
    return -1;
}

void test_case(const std::string& ver_s, const std::wstring& version, int expectedBiocMinor) {
    int major = ParseMajorVersion(version);
    int minor = ParseMinorVersion(version);
    int bioc = InferBiocVersion(major, minor);
    std::cout << "Version: " << ver_s 
              << " -> Major: " << major 
              << ", Minor: " << minor 
              << ", Infer Bioc: 3." << bioc
              << " (Expected: 3." << expectedBiocMinor << ")" << std::endl;
    assert(bioc == expectedBiocMinor);
}

int main() {
    try {
        test_case("1.50.5", L"1.50.5", 18);
        test_case("1.52.0", L"1.52.0", 19);
        test_case("1.54.2", L"1.54.2", 20);
        test_case("2.0.1", L"2.0.1", 21);
        test_case("2.2.0", L"2.2.0", 22);
        test_case("1.48.0", L"1.48.0", 7);
        std::cout << "\nAll test cases passed successfully!" << std::endl;
        return 0;
    } catch (...) {
        std::cerr << "Test failed with exception!" << std::endl;
        return 1;
    }
}
