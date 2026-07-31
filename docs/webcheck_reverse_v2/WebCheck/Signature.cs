using System;
using System.IO;
using System.Threading;
using System.Windows.Forms;
using EUSignCP;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

internal class Signature
{
	private bool Blok;

	private IntPtr KeyDs;

	private IntPtr KeyPrt;

	private string KeyPasT;

	private string KeyPathT;

	public Signature()
	{
		Blok = false;
		KeyPasT = "ПарольПоУмолчаниюДляСравнения:)";
		KeyPathT = "ПутьПоУмолчаниюДляСравнения:)";
	}

	internal bool SignatureStart()
	{
		if (!IEUSignCP.IsInitialized())
		{
			ReIni();
		}
		int retriesPrt = All.RetriesPrt;
		for (int i = 0; i <= retriesPrt; i = checked(i + 1))
		{
			if (!IEUSignCP.IsInitialized())
			{
				if (!SignatureInitialized())
				{
					ReIni();
					Thread.Sleep(All.A.Delay);
				}
				continue;
			}
			return true;
		}
		return false;
	}

	private void ReIni()
	{
		string text = All.MyDoc() + "\\WebCheck\\osplm.ini";
		string text2 = All.MyDoc() + "\\WebCheck\\reserve.ini";
		try
		{
			if (File.Exists(text))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			}
			Application.DoEvents();
			if (File.Exists(text2))
			{
				MyProject.Computer.FileSystem.CopyFile(text2, text);
			}
			Application.DoEvents();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private bool SignatureInitialized()
	{
		bool result;
		try
		{
			if (!IEUSignCP.IsInitialized())
			{
				IEUSignCP.SetSettingsFilePath(All.MyDoc() + "\\WebCheck\\");
				Application.DoEvents();
				int num = IEUSignCP.Initialize();
				Application.DoEvents();
				if (All.A.AcskSettings > 0)
				{
					SetServer();
				}
				result = num <= 0;
				goto IL_005d;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_005d;
		}
		result = true;
		goto IL_005d;
		IL_005d:
		return result;
	}

	public bool SignatureStop()
	{
		KeyDs = default(IntPtr);
		bool result;
		try
		{
			if (IEUSignCP.IsInitialized())
			{
				Application.DoEvents();
				IEUSignCP.Finalize();
				Application.DoEvents();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0036;
		}
		result = true;
		goto IL_0036;
		IL_0036:
		return result;
	}

	internal void SetServer()
	{
		TypServera typServera = Servers(All.A.AcskSettings);
		string address = "";
		string port = "";
		IEUSignCP.GetOCSPSettings(out var _, out var _, out address, out port);
		if (Operators.CompareString(typServera.OCSP.Trim().ToLower(), address.Trim().ToLower(), TextCompare: false) != 0)
		{
			IEUSignCP.SetOCSPSettings(useOCSP: true, beforeStore: false, typServera.OCSP, typServera.OCSPport);
			IEUSignCP.SetTSPSettings(All.A.UseACSKTSPserver, typServera.TSP, typServera.TSPport);
			IEUSignCP.SetCMPSettings(useCMP: true, typServera.CMP, typServera.CMPport, "");
		}
	}

	internal TypServera Servers(int index)
	{
		TypServera result = default(TypServera);
		result.CMP = "acsk.privatbank.ua";
		result.CMPport = "80";
		result.OCSP = "acsk.privatbank.ua/services/ocsp/";
		result.OCSPport = "80";
		result.TSP = "acsk.privatbank.ua";
		result.TSPport = "80";
		result.Name = "privatbank";
		result.Index = 4;
		result.Count = 11;
		if (index > result.Count)
		{
			index = 0;
		}
		switch (index)
		{
		case 0:
			result.TSP = "";
			result.OCSP = "";
			result.CMP = "";
			result.Name = "ospus.ini";
			result.Index = 0;
			break;
		case 1:
			result.TSP = "tsp.masterkey.ua/services/tsp/";
			result.OCSP = "ocsp.masterkey.ua/services/ocsp/";
			result.CMP = "masterkey.ua";
			result.Name = "masterkey";
			result.Index = 1;
			break;
		case 2:
			result.TSP = "uakey.com.ua";
			result.OCSP = "uakey.com.ua/services/ocsp/";
			result.CMP = "uakey.com.ua";
			result.Name = "uakey";
			result.Index = 2;
			break;
		case 3:
			result.TSP = "ca.ksystems.com.ua";
			result.OCSP = "ca.ksystems.com.ua/services/ocsp/";
			result.CMP = "ca.ksystems.com.ua";
			result.Name = "ksystems";
			result.Index = 3;
			break;
		case 4:
			result.TSP = "acsk.privatbank.ua";
			result.OCSP = "acsk.privatbank.ua/services/ocsp/";
			result.CMP = "acsk.privatbank.ua";
			result.Name = "privatbank";
			result.Index = 4;
			break;
		case 5:
			result.TSP = "csk.ukrsibbank.com";
			result.OCSP = "csk.ukrsibbank.com/services/ocsp/";
			result.CMP = "csk.ukrsibbank.com";
			result.Name = "ukrsibbank";
			result.Index = 5;
			break;
		case 6:
			result.TSP = "acskidd.gov.ua/services/tsp/";
			result.OCSP = "acskidd.gov.ua/services/ocsp/";
			result.CMP = "acskidd.gov.ua";
			result.Name = "acskidd";
			result.Index = 6;
			break;
		case 7:
			result.TSP = "ca.informjust.ua/services/tsp/";
			result.OCSP = "ca.informjust.ua/services/ocsp/";
			result.CMP = "ca.informjust.ua";
			result.Name = "diia";
			result.Index = 7;
			break;
		case 8:
			result.TSP = "csk.uss.gov.ua/services/tsp";
			result.OCSP = "csk.uss.gov.ua/services/ocsp";
			result.CMP = "csk.uss.gov.ua/services/cmp";
			result.Name = "УСС";
			result.Index = 8;
			break;
		case 9:
			result.TSP = "ca.depositsign.com/services/tsp/";
			result.OCSP = "ca.depositsign.com/services/ocsp/";
			result.CMP = "ca.depositsign.com/services/cmp/";
			result.Name = "ДЕПОЗИТ САЙН";
			result.Index = 9;
			break;
		case 10:
			result.TSP = "ca.monobank.ua/services/tsp/";
			result.OCSP = "ca.monobank.ua/services/ocsp/";
			result.CMP = "ca.monobank.ua/services/cmp/";
			result.Name = "MONOBANK";
			result.Index = 10;
			break;
		case 11:
			result.TSP = "ca.vchasno.ua/services/tsp/";
			result.OCSP = "ca.vchasno.ua/services/ocsp/";
			result.CMP = "ca.vchasno.ua/services/cmp/";
			result.Name = "ВЧАСНО АЦСК";
			result.Index = 11;
			break;
		}
		return result;
	}

	internal TypErr SignatureFile(string fileKeyS, string PasSport, string fileXMLs)
	{
		if (All.A.CheckTimeLog)
		{
			All.Lg.SaveTextToLog("TimeControl", "Вход в подпись");
		}
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		TypErr result;
		if (Blok)
		{
			typErr.errCode = 1016;
			typErr.errStr = "Внимание Ошибка! Опасная ситуация, повторный запуск подписи файла.";
			Blok = false;
			result = typErr;
		}
		else
		{
			Blok = true;
			TypErrIntPrt typErrIntPrt = default(TypErrIntPrt);
			typErrIntPrt.errCode = 0;
			typErrIntPrt.errStr = "";
			try
			{
				SignatureStart();
				int retriesPrt = All.RetriesPrt;
				for (int i = 1; i <= retriesPrt; i = checked(i + 1))
				{
					typErrIntPrt = LoadKey(fileKeyS, PasSport);
					if (typErrIntPrt.errCode == 0)
					{
						break;
					}
					if (i == 9 || i == 18 || i == 27)
					{
						SignatureStop();
					}
					Thread.Sleep(All.A.Delay);
				}
				if (typErrIntPrt.errCode > 0)
				{
					typErr.errCode = typErrIntPrt.errCode;
					typErr.errStr = typErrIntPrt.errStr;
					IEUSignCP.CtxFreePrivateKey(KeyDs);
					IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
					KeyDs = default(IntPtr);
					Blok = false;
					result = typErr;
					goto IL_034e;
				}
				typErr = FileSignature(fileXMLs);
				if (typErr.errCode == 32)
				{
					if (!All.l.OfflineTrue())
					{
						if (All.A.FullVersion)
						{
							if (All.OfflineAllowed)
							{
								TypErr typErr2 = All.OfflineOn();
								if (typErr2.errCode > 0)
								{
									typErr.errCode = typErr2.errCode;
									typErr.errStr = typErr2.errStr;
									IEUSignCP.CtxFreePrivateKey(KeyDs);
									IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
									KeyDs = default(IntPtr);
									Blok = false;
									result = typErr;
								}
								else
								{
									typErr.errCode = 32;
									typErr.errStr = "Включен офлайн режим";
									IEUSignCP.CtxFreePrivateKey(KeyDs);
									IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
									KeyDs = default(IntPtr);
									Blok = false;
									result = typErr;
								}
							}
							else
							{
								typErr.errCode = 33;
								typErr.errStr = "Переход в офлайн режим запрещен";
								IEUSignCP.CtxFreePrivateKey(KeyDs);
								IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
								KeyDs = default(IntPtr);
								Blok = false;
								result = typErr;
							}
						}
						else
						{
							typErr.errCode = 34;
							typErr.errStr = "Немає інтернету, перехід в офлайн режим неможливий, безкоштовна версія";
							IEUSignCP.CtxFreePrivateKey(KeyDs);
							IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
							KeyDs = default(IntPtr);
							Blok = false;
							result = typErr;
						}
						goto IL_034e;
					}
				}
				else if (typErr.errCode > 0)
				{
					IEUSignCP.CtxFreePrivateKey(KeyDs);
					IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
					KeyDs = default(IntPtr);
					Blok = false;
					result = typErr;
					goto IL_034e;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErr.errCode = 1012;
				typErr.errStr = "Ошибка инициализации процедуры подписи файла";
				IEUSignCP.CtxFreePrivateKey(KeyDs);
				IEUSignCP.CtxFree(typErrIntPrt.ReturnIntPrt);
				KeyDs = default(IntPtr);
				Blok = false;
				result = typErr;
				ProjectData.ClearProjectError();
				goto IL_034e;
			}
			Blok = false;
			if (All.A.CheckTimeLog)
			{
				All.Lg.SaveTextToLog("TimeControl", "Вsход в подписи");
			}
			result = typErr;
		}
		goto IL_034e;
		IL_034e:
		return result;
	}

	private TypErrIntPrt LoadKey(string fileKeyS, string PasSport)
	{
		TypErrIntPrt result = default(TypErrIntPrt);
		result.errCode = 0;
		result.errStr = "";
		if (All.A.PathKey.Trim().Length > 3)
		{
			fileKeyS = All.A.PathKey;
		}
		if (All.A.PassKey.Trim().Length > 0)
		{
			PasSport = All.A.PassKey;
		}
		if (!File.Exists(fileKeyS))
		{
			result.errCode = 49;
			result.errStr = "Помилка, немає файлу ключа.";
			return result;
		}
		string text = fileKeyS;
		string text2 = PasSport;
		SignatureStart();
		IEUSignCP.EU_CERT_OWNER_INFO certOwnerInfo = default(IEUSignCP.EU_CERT_OWNER_INFO);
		int num = IEUSignCP.CtxCreate(out var context);
		Application.DoEvents();
		if (num != 0)
		{
			num = IEUSignCP.CtxCreate(out context);
			if (num != 0)
			{
				result.errStr = "Ошибка получения IntPtr для загрузки ключа: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				result.errCode = 24;
				return result;
			}
			result.errCode = 0;
			result.errStr = "";
		}
		else
		{
			result.errCode = 0;
			result.errStr = "";
		}
		if ((Operators.CompareString(KeyPasT, text2, TextCompare: false) != 0) | (Operators.CompareString(KeyPathT, text, TextCompare: false) != 0))
		{
			KeyDs = default(IntPtr);
		}
		if (KeyDs == default(IntPtr))
		{
			if (All.A.CheckTimeLog)
			{
				All.Lg.SaveTextToLog("TimeControl", "Старт загрузки ключа");
			}
			num = IEUSignCP.CtxReadPrivateKeyFile(context, text, text2, out KeyDs, out certOwnerInfo);
			KeyPasT = text2;
			KeyPathT = text;
			if (All.A.CheckTimeLog)
			{
				All.Lg.SaveTextToLog("TimeControl", "Финишь загрузки ключа");
			}
			if (num != 0)
			{
				result.errStr = "Помилка отримання ключа для підпису файлу: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				result.errCode = 1010;
			}
			else
			{
				result.errCode = 0;
				result.errStr = "";
			}
			KeyPrt = context;
			result.ReturnIntPrt = context;
			return result;
		}
		if (All.A.CheckTimeLog)
		{
			All.Lg.SaveTextToLog("TimeControl", "Загрузка ключа не требуется");
		}
		result.errCode = 0;
		result.errStr = "";
		result.ReturnIntPrt = KeyPrt;
		return result;
	}

	internal TypErrStrCert Cert(string KeyF, string PasS)
	{
		TypErrStrCert result = default(TypErrStrCert);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		result.ReturnStart = "";
		result.ReturnEnd = "";
		result.ReturnSerial = "";
		result.ReturnIssuer = "";
		result.ReturnOpenShift = true;
		result.ReturnTIN = "";
		result.ReturnSUBJCN = "";
		SignatureStart();
		try
		{
			if (!File.Exists(KeyF))
			{
				result.errCode = 49;
				result.errStr = "Ошибкаю. Файл ключа не найден";
				result.ReturnOpenShift = false;
				return result;
			}
			IEUSignCP.EU_CERT_OWNER_INFO certOwnerInfo = default(IEUSignCP.EU_CERT_OWNER_INFO);
			IEUSignCP.EU_CERT_INFO_EX certInfoEx = default(IEUSignCP.EU_CERT_INFO_EX);
			SetServer();
			int num = IEUSignCP.ReadPrivateKeyFile(KeyF, PasS, out certOwnerInfo);
			if (num != 0)
			{
				result.errCode = 65;
				result.errStr = "Ошибкаю при загрузке ключа для проверке сертификата: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				result.ReturnOpenShift = false;
				return result;
			}
			num = IEUSignCP.GetCertificateInfoEx(certOwnerInfo.issuer, certOwnerInfo.serial, out certInfoEx);
			if (num != 0)
			{
				result.errCode = 65;
				result.errStr = "Ошибкаю при проверке сертификата: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				result.ReturnOpenShift = false;
				return result;
			}
			ref IEUSignCP.SYSTEMTIME certBeginTime = ref certInfoEx.certBeginTime;
			result.ReturnStart = certBeginTime.wDay + "." + certBeginTime.wMonth + "." + certBeginTime.wYear + " ";
			ref string returnStart = ref result.ReturnStart;
			ref string reference = ref returnStart;
			returnStart = reference + certBeginTime.wHour + ":" + certBeginTime.wMinute + ":" + certBeginTime.wSecond;
			ref IEUSignCP.SYSTEMTIME certEndTime = ref certInfoEx.certEndTime;
			result.ReturnEnd = certEndTime.wDay + "." + certEndTime.wMonth + "." + certEndTime.wYear + " ";
			ref string returnEnd = ref result.ReturnEnd;
			reference = ref returnEnd;
			returnEnd = reference + certEndTime.wHour + ":" + certEndTime.wMinute + ":" + certEndTime.wSecond;
			result.ReturnSerial = certInfoEx.serial;
			result.ReturnIssuer = certInfoEx.issuer;
			result.ReturnSUBJCN = certInfoEx.subjCN;
			string text = certInfoEx.subjEDRPOUCode.Trim();
			if (Operators.CompareString(text, "", TextCompare: false) == 0)
			{
				text = certInfoEx.subjDRFOCode;
			}
			result.ReturnTIN = text;
			result.ReturnOpenShift = KeyShiftTime(result.ReturnEnd);
			result.ReturnStr = Strings.Replace(certInfoEx.publicKeyID, " ", "");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 65;
			result.errStr = "Ошибка при проверке сертификата!";
			result.ReturnStr = "";
			result.ReturnStart = "";
			result.ReturnEnd = "";
			result.ReturnSerial = "";
			result.ReturnIssuer = "";
			result.ReturnOpenShift = false;
			result.ReturnTIN = "";
			result.ReturnSUBJCN = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr CertDel(string KeyF, string PasS)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		SignatureStart();
		try
		{
			if (!File.Exists(KeyF))
			{
				result.errCode = 49;
				result.errStr = "Ошибкаю. Файл ключа не найден";
				return result;
			}
			IEUSignCP.EU_CERT_OWNER_INFO certOwnerInfo = default(IEUSignCP.EU_CERT_OWNER_INFO);
			IEUSignCP.EU_CERT_INFO_EX certInfo = default(IEUSignCP.EU_CERT_INFO_EX);
			int num = IEUSignCP.ReadPrivateKeyFile(KeyF, PasS, out certOwnerInfo);
			if (num != 0)
			{
				result.errCode = 65;
				result.errStr = "Ошибкаю при загрузке ключа для удаления сертификата: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				return result;
			}
			if (IEUSignCP.DeleteCertificate(certOwnerInfo.issuer, certOwnerInfo.serial) != 0)
			{
				All.Lg.SaveTextToLog("CertDel", "Ошибка удаление сертификата индекс 0");
			}
			if (IEUSignCP.EnumOwnCertificates(1, out certInfo) == 0 && IEUSignCP.DeleteCertificate(certInfo.issuer, certInfo.serial) != 0)
			{
				All.Lg.SaveTextToLog("CertDel", "Ошибка удаление сертификата индекс 1");
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 65;
			result.errStr = "Ошибка при удалении сертификата!  " + ex2.Message;
			All.Lg.SaveTextToLog("CertDel", result.errStr);
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr CertDel(string tINN)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		SignatureStart();
		try
		{
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini");
			string issuer = iniHGB.GetString(tINN, "Issuer").Trim();
			string serial = iniHGB.GetString(tINN, "Serial").Trim();
			iniHGB.WriteString(tINN, "Updated", "0");
			int num = IEUSignCP.DeleteCertificate(issuer, serial);
			if (num != 0)
			{
				result.errCode = 65;
				result.errStr = "Ошибкаю при удалении сертификата: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
				All.Lg.SaveTextToLog("CertDel", "Ошибка удаление сертификата №" + result.errCode, result.errStr);
				return result;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 65;
			result.errStr = "Ошибка при проверке сертификата!";
			All.Lg.SaveTextToLog("CertDel", "Неопеделенная ошибка при удалении сертификата");
			ProjectData.ClearProjectError();
		}
		All.Lg.SaveTextToLog("CertDel", "Сертификат успешно удален");
		return result;
	}

	private bool KeyShiftTime(string ds)
	{
		bool result;
		try
		{
			DateTime now = DateTime.Now;
			DateTime date = Conversions.ToDate(ds);
			result = ((checked((int)DateAndTime.DateDiff(DateInterval.Minute, now, date)) > All.A.LimitCertificate) ? true : false);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool KeyShiftTimeINN(string INN)
	{
		bool result;
		if (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0)
		{
			result = true;
		}
		else
		{
			try
			{
				DateTime now = DateTime.Now;
				DateTime date = Conversions.ToDate(new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini").GetString(INN, "EndKey").Trim());
				result = ((checked((int)DateAndTime.DateDiff(DateInterval.Minute, now, date)) > All.A.LimitCertificate) ? true : false);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = true;
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	internal string KeyShiftEnd(string INN)
	{
		return new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini").GetString(INN, "EndKey").Trim();
	}

	private TypErr FileSignature(string fileXMLs)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		SignatureStart();
		string fileNameWithSign = fileXMLs + ".p7s";
		if (KeyDs == default(IntPtr))
		{
			result.errCode = 71;
			result.errStr = "Ошибка подписи файла. KeyDs не присвоен.";
			return result;
		}
		int num = IEUSignCP.CtxSignFile(KeyDs, 1, fileXMLs, external: false, appendCert: true, fileNameWithSign);
		Application.DoEvents();
		switch (num)
		{
		case 65:
			result.errCode = 32;
			result.errStr = "Переход в офлайн режим. Ошибка при подписи XML файла: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
			return result;
		default:
			result.errCode = 1011;
			result.errStr = "Ошибка при подписи XML файла: " + IEUSignCP.GetErrorDesc(num) + " (" + num + ")";
			break;
		case 0:
			result.errCode = 0;
			result.errStr = "";
			break;
		}
		return result;
	}

	internal void ErrorShow(bool ShowWindows)
	{
		IEUSignCP.SetUIMode(ShowWindows);
	}

	internal TypErrStr DecodCheck(string codeStr, string pathFile = "")
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErrStr result;
		if (Blok)
		{
			typErrStr.errCode = 1016;
			typErrStr.errStr = "Внимание Ошибка! Опасная ситуация, повторный запуск расшифровки файла.";
			Blok = false;
			result = typErrStr;
		}
		else
		{
			Blok = true;
			if (!SignatureStart())
			{
				typErrStr.errCode = 54;
				typErrStr.errStr = "Ошибка подключения IEUSignCP для декодирования";
				result = typErrStr;
			}
			else
			{
				byte[] data;
				IEUSignCP.EU_SIGN_INFO signInfo;
				int num = IEUSignCP.VerifyDataInternal(codeStr, out data, out signInfo);
				if (num != 0)
				{
					typErrStr.errCode = 54;
					typErrStr.errStr = "Ошибка декодирования IEUSignCP: " + IEUSignCP.GetErrorDesc(num);
					All.Lg.SaveTextToLog("DecodCheck", "Ошибка декодирования IEUSignCP", "IEUSignCP код ошибки " + IEUSignCP.GetErrorDesc(num));
					Blok = false;
					result = typErrStr;
				}
				else
				{
					string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
					if (pathFile.Trim().Length > 0)
					{
						text = pathFile;
					}
					try
					{
						if (File.Exists(text))
						{
							Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
							Application.DoEvents();
						}
						File.WriteAllBytes(text, data);
						Application.DoEvents();
					}
					catch (Exception ex)
					{
						ProjectData.SetProjectError(ex);
						Exception ex2 = ex;
						typErrStr.errCode = 53;
						typErrStr.errStr = "Ошибка сохранение файла с декодированной строкой.";
						Blok = false;
						result = typErrStr;
						ProjectData.ClearProjectError();
						goto IL_01ba;
					}
					string text2 = "";
					StreamReader streamReader = new StreamReader(text);
					do
					{
						text2 += streamReader.ReadLine();
					}
					while (!streamReader.EndOfStream);
					Application.DoEvents();
					streamReader.Close();
					typErrStr.errCode = 0;
					typErrStr.errStr = "";
					typErrStr.ReturnStr = text2;
					Blok = false;
					result = typErrStr;
				}
			}
		}
		goto IL_01ba;
		IL_01ba:
		return result;
	}
}
