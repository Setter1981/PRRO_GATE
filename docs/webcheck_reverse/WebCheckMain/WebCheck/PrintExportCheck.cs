using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Drawing.Printing;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using System.Xml;
using Gma.QrCodeNet.Encoding.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using iTextSharp.text;
using iTextSharp.text.pdf;

namespace WebCheck;

internal class PrintExportCheck
{
	public string nPrint;

	internal int Dlstr;

	private string[] StrCheck;

	private string[] StrCheckR;

	private string[] StrCheckN;

	private bool Zapolnili;

	private string LincWWW;

	private string DataWWW;

	private string TimeWWW;

	private int TypWWW;

	internal string Tb1;

	internal string Tb2;

	internal string Tb;

	[CompilerGenerated]
	[AccessedThroughProperty("PrintDocument1")]
	private PrintDocument _PrintDocument1;

	private Image ImageQRt;

	private Image PrintLogo;

	private PrintDialog PrintDialog1;

	private bool ExportXML;

	private string MacPr;

	private string DataTimePr;

	private string FiChPr;

	private string SumPr;

	private string FnPr;

	private virtual PrintDocument PrintDocument1
	{
		[CompilerGenerated]
		get
		{
			return _PrintDocument1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			PrintPageEventHandler val = new PrintPageEventHandler(PrintDocument1_PrintPage);
			PrintDocument printDocument = _PrintDocument1;
			if (printDocument != null)
			{
				printDocument.PrintPage -= val;
			}
			_PrintDocument1 = value;
			printDocument = _PrintDocument1;
			if (printDocument != null)
			{
				printDocument.PrintPage += val;
			}
		}
	}

	public PrintExportCheck()
	{
		//IL_005b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0065: Expected O, but got Unknown
		//IL_0066: Unknown result type (might be due to invalid IL or missing references)
		//IL_0070: Expected O, but got Unknown
		Dlstr = 29;
		StrCheck = new string[3];
		StrCheckR = new string[3];
		StrCheckN = new string[1];
		Zapolnili = false;
		LincWWW = "https://cabinet.tax.gov.ua/cashregs/check?id=";
		DataWWW = "";
		TimeWWW = "";
		PrintDocument1 = new PrintDocument();
		PrintDialog1 = new PrintDialog();
		ExportXML = false;
	}

	internal string CheckVis(string NumCheckVis, bool id = false)
	{
		Zapolnili = false;
		nPrint = NumCheckVis;
		ResCheck();
		LincWWW = "https://cabinet.tax.gov.ua/cashregs/check?id=" + Tb2 + "&date=" + DataWWW + "&time=" + TimeWWW + "&fn=" + FnPr + "&sm=" + SumPr;
		if (Operators.CompareString(MacPr, (string)null, false) != 0 && MacPr.Length > 1)
		{
			string text = "&mac=" + MacPr;
			LincWWW += text;
		}
		return LincWWW;
	}

	internal void PrintCheck(string NumCheckVis, Image ImageQR, Image ImageLogo)
	{
		//IL_00b7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00bd: Invalid comparison between Unknown and I4
		ImageQRt = ImageQR;
		PrintLogo = ImageLogo;
		if (All.f.IntegerGetFn(All.A.FN, "PrinterWidth") == 80)
		{
			Dlstr = 40;
		}
		else
		{
			Dlstr = 29;
		}
		Zapolnili = false;
		nPrint = NumCheckVis;
		ResCheck();
		if (Operators.CompareString(All.A.PrinterName, "", false) != 0)
		{
			PrintDialog1.PrinterSettings.PrinterName = All.A.PrinterName;
		}
		PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
		try
		{
			PrintDocument1.Print();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			if ((int)((CommonDialog)PrintDialog1).ShowDialog() == 1)
			{
				All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
				All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
				PrintDocument1.Print();
			}
			ProjectData.ClearProjectError();
		}
	}

	private void PrintDocument1_PrintPage(object sender, PrintPageEventArgs e)
	{
		//IL_00e0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f6: Expected O, but got Unknown
		//IL_038f: Unknown result type (might be due to invalid IL or missing references)
		//IL_03a5: Expected O, but got Unknown
		//IL_0362: Unknown result type (might be due to invalid IL or missing references)
		//IL_0378: Expected O, but got Unknown
		//IL_02fb: Unknown result type (might be due to invalid IL or missing references)
		//IL_0311: Expected O, but got Unknown
		//IL_032b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0341: Expected O, but got Unknown
		//IL_029b: Unknown result type (might be due to invalid IL or missing references)
		//IL_02b1: Expected O, but got Unknown
		//IL_02cb: Unknown result type (might be due to invalid IL or missing references)
		//IL_02e1: Expected O, but got Unknown
		//IL_03bf: Unknown result type (might be due to invalid IL or missing references)
		//IL_03d5: Expected O, but got Unknown
		int num = 0;
		string text = All.f.GetString("Global", "QrCode");
		int num2;
		try
		{
			if (Dlstr == 29)
			{
				e.Graphics.DrawImage(PrintLogo, 3, 3, 174, 45);
			}
			else
			{
				e.Graphics.DrawImage(PrintLogo, 36, 3, 174, 45);
			}
			num2 = 53;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			num2 = 0;
			ProjectData.ClearProjectError();
		}
		checked
		{
			int num3 = StrCheckN.Count() - 1;
			for (int i = 0; i <= num3; i++)
			{
				if (Operators.CompareString(StrCheckN[i], (string)null, false) == 0)
				{
					StrCheckN[i] = "";
				}
				if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", false) != 0)
				{
					num = i * 12 + num2;
					e.Graphics.DrawString(StrCheckN[i], new Font("Consolas", 8f), Brushes.Black, 0f, (float)num);
				}
				else
				{
					if (!((TypWWW < 3) | (TypWWW == 8)))
					{
						continue;
					}
					num = i * 12 + 6 + num2;
					if (Dlstr == 29)
					{
						if (Operators.CompareString(text, "0", false) == 0)
						{
							e.Graphics.DrawImage(ImageQRt, 36, num, 118, 118);
						}
						else if (Operators.CompareString(text, "1", false) == 0)
						{
							e.Graphics.DrawImage(ImageQRt, 9, num, 172, 172);
						}
						else
						{
							e.Graphics.DrawImage(ImageQRt, 36, num, 118, 118);
							All.f.WriteString("Global", "QrCode", "0");
						}
					}
					else if (Operators.CompareString(text, "0", false) == 0)
					{
						e.Graphics.DrawImage(ImageQRt, 69, num, 118, 118);
					}
					else if (Operators.CompareString(text, "1", false) == 0)
					{
						e.Graphics.DrawImage(ImageQRt, 45, num, 172, 172);
					}
					else
					{
						text = "0";
						e.Graphics.DrawImage(ImageQRt, 69, num, 118, 118);
						All.f.WriteString("Global", "QrCode", text);
					}
					num2 = ((Operators.CompareString(text, "0", false) != 0) ? (num2 + 172) : (num2 + 118));
				}
			}
			if (!All.A.FullVersion)
			{
				if (Dlstr == 29)
				{
					e.Graphics.DrawString("Безкоштовна версія ПРРО 'ВебЧек'", new Font("Consolas", 7f), Brushes.Black, 5f, (float)num);
					num += 10;
					e.Graphics.DrawString("http://www.webchek.com.ua", new Font("Consolas", 7f), Brushes.Black, 18f, (float)num);
				}
				else
				{
					e.Graphics.DrawString("Безкоштовна версія ПРРО 'ВебЧек'", new Font("Consolas", 7f), Brushes.Black, 35f, (float)num);
					num += 10;
					e.Graphics.DrawString("http://www.webchek.com.ua", new Font("Consolas", 7f), Brushes.Black, 48f, (float)num);
				}
			}
			else if (Dlstr == 29)
			{
				e.Graphics.DrawString("ПРРО  'ВебЧек'", new Font("Consolas", 7f), Brushes.Black, 54f, (float)num);
			}
			else
			{
				e.Graphics.DrawString("ПРРО  'ВебЧек'", new Font("Consolas", 7f), Brushes.Black, 93f, (float)num);
			}
			num += 49;
			e.Graphics.DrawString(".", new Font("Consolas", 7f), Brushes.Gray, 5f, (float)num);
		}
	}

	internal bool ExportCheckToPDF(string PathPDF, string CheckXML, string CheckName, bool lite = true)
	{
		if (lite)
		{
			ExportXML = true;
		}
		Zapolnili = false;
		CheckName = CheckName.Trim();
		CheckName = CheckName.Replace("`", "_");
		CheckXML = CheckXML.ToLower();
		Tb2 = CheckName;
		Dlstr = 30;
		bool result;
		try
		{
			ResCheck(CheckXML);
			result = ((!lite) ? ExportToPDFnew(PathPDF, CheckName) : ExportToPDF(PathPDF));
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

	internal void ExportToFile(string CheckXML, string CheckName)
	{
		ExportXML = true;
		Zapolnili = false;
		CheckName = CheckName.Trim();
		CheckName = CheckName.Replace("`", "_");
		Tb2 = CheckName;
		nPrint = CheckName;
		if (All.A.ExportLength < 18)
		{
			All.A.ExportLength = 18;
		}
		Dlstr = All.A.ExportLength;
		string path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\";
		if (!Directory.Exists(path))
		{
			Directory.CreateDirectory(path);
		}
		if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0)
		{
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\" + DateTime.Now.Year + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\" + DateTime.Now.Year + "\\" + DateTime.Now.Month + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\" + DateTime.Now.Year + "\\" + DateTime.Now.Month + "\\" + DateTime.Now.Day + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
		}
		else
		{
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + DateTime.Now.Year + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + DateTime.Now.Year + "\\" + DateTime.Now.Month + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
			path = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + DateTime.Now.Year + "\\" + DateTime.Now.Month + "\\" + DateTime.Now.Day + "\\";
			if (!Directory.Exists(path))
			{
				Directory.CreateDirectory(path);
			}
		}
		try
		{
			Secondary.SendMail[0] = "";
			Secondary.SendMail[1] = "";
			Secondary.SendMail[2] = "";
			Secondary.SendMail[3] = "";
			Secondary.SendMail[4] = "";
			Secondary.SendMail[5] = "";
			ResCheck(CheckXML);
			ExportToArrayS();
			if (All.A.SendToEmail && Secondary.SendMail[0].Trim().Length > 3)
			{
				SendingMail sendingMail = new SendingMail();
				string text = "Електронна копія чека\r\n";
				text += "Ви отримали цей лист, так як повідомили адресу електронної пошти або вказали його при покупці в інтернеті. Питання про чек можете задати продавцю  - реквізити вказані в чеку.\r\n";
				text += "Посилання на чек на сайті фіскальної служби:\r\n";
				text = text + Secondary.SendMail[4] + "\r\n";
				text += "\r\n";
				text = text + Secondary.SendMail[1] + "\r\n";
				text += "\r\n";
				text += "Дякуємо за покупку!\r\n";
				if (!sendingMail.SendMail(Body: text + "Для реєстрації чеків використовується ВебЧек: ПРРО", ini: true, ToMail: Secondary.SendMail[0], Tema: "Електронна копія чека " + Secondary.SendMail[2] + " від " + Secondary.SendMail[5]))
				{
					All.Lg.SaveTextToLog("Ошибка отправки чека", "eMail " + Secondary.SendMail[0], "Чек номер " + Secondary.SendMail[2]);
				}
			}
			if (All.A.ToPDF)
			{
				if (File.Exists(path + CheckName + ".pdf"))
				{
					FileSystem.DeleteFile(path + CheckName + ".pdf");
					Application.DoEvents();
				}
				ResCheck(CheckXML);
				ExportToPDF(path + CheckName + ".pdf");
				Application.DoEvents();
			}
			if (All.A.ToXML)
			{
				if (File.Exists(path + CheckName + ".xml"))
				{
					FileSystem.DeleteFile(path + CheckName + ".xml");
					Application.DoEvents();
				}
				XmlDocument xmlDocument = new XmlDocument();
				xmlDocument.LoadXml(CheckXML);
				xmlDocument.Save(path + CheckName + ".xml");
				Application.DoEvents();
			}
			if (All.A.ToTXT)
			{
				if (File.Exists(path + CheckName + ".txt"))
				{
					FileSystem.DeleteFile(path + CheckName + ".txt");
					Application.DoEvents();
				}
				ResCheck(CheckXML);
				ExportToTXT(path + CheckName + ".txt");
				Application.DoEvents();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	internal void ExportToArray(string CheckXML, string CheckName)
	{
		ExportXML = true;
		Zapolnili = false;
		CheckName = CheckName.Trim();
		CheckName = CheckName.Replace("`", "_");
		CheckXML = CheckXML.ToLower();
		Tb2 = CheckName;
		nPrint = CheckName;
		if (All.A.ExportLength < 18)
		{
			All.A.ExportLength = 18;
		}
		Dlstr = All.A.ExportLength;
		try
		{
			Secondary.SendMail[0] = "";
			Secondary.SendMail[1] = "";
			Secondary.SendMail[2] = "";
			Secondary.SendMail[3] = "";
			Secondary.SendMail[4] = "";
			Secondary.SendMail[5] = "";
			ResCheck(CheckXML);
			ExportToArrayS();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void ResCheck(string CheckXML = "")
	{
		if (!Zapolnili)
		{
			Fill(CheckXML);
		}
		TransferData();
		Tb = "";
		checked
		{
			int num = StrCheckN.Count() - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(StrCheckN[i], (string)null, false) == 0)
				{
					StrCheckN[i] = "";
				}
				if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", false) != 0)
				{
					ref string tb = ref Tb;
					tb = tb + StrCheckN[i] + "\r\n";
				}
			}
		}
	}

	private void Fill(string CheckXML = "")
	{
		Zapolnili = true;
		TypPrintChecks typPrintChecks = default(TypPrintChecks);
		typPrintChecks.ReturnStr = "";
		typPrintChecks.ReturnStrN = "";
		typPrintChecks.ReturnStrTaxN = "";
		typPrintChecks.ReturnOffline = "";
		typPrintChecks.ReturnMac = "";
		typPrintChecks.ReturnID = "";
		typPrintChecks.ReturnSum = "";
		checked
		{
			if (Operators.CompareString(CheckXML.Trim(), "", false) == 0)
			{
				typPrintChecks = ((Operators.CompareString(nPrint.ToLower(), "z", false) == 0) ? All.Rf.CheckXMLz() : (Versioned.IsNumeric((object)nPrint) ? All.Rf.CheckXMLNumber(nPrint, SearchID: true) : ((Operators.CompareString(nPrint.Trim(), "", false) == 0) ? All.Rf.CheckXMLNumber(nPrint) : (((Operators.CompareString(Conversions.ToString(nPrint[nPrint.Length - 1]), "Z", false) == 0) & (nPrint.Length < 9)) ? All.Rf.CheckXMLNumber(nPrint.ToUpper()) : ((!((Operators.CompareString(Conversions.ToString(nPrint[nPrint.Length - 1]), "z", false) == 0) & (nPrint.Length < 9))) ? All.Rf.CheckXMLNumberTax(nPrint) : All.Rf.CheckXMLNumber(nPrint.ToUpper()))))));
			}
			else
			{
				typPrintChecks.ReturnStr = CheckXML;
			}
			Tb1 = typPrintChecks.ReturnStrN;
			if (!ExportXML)
			{
				Tb2 = typPrintChecks.ReturnStrTaxN;
			}
			Application.DoEvents();
			int num = 0;
			if (Operators.CompareString(nPrint, "LastX", false) == 0)
			{
				num = 4;
				Tb1 = "X ЗВІТ";
				Tb2 = "ЗМІНА № " + All.l.ReturnOpenShift().ReturnStr;
			}
			else if (Operators.CompareString(nPrint, "pZvit", false) == 0)
			{
				num = 5;
				Tb1 = "ЗВЕДЕНИЙ ЗВІТ";
				Tb2 = "СЛУЖБОВИЙ ЧЕК";
			}
			else
			{
				num = TypChekcs(typPrintChecks.ReturnStr);
				StrCheck = new string[3];
				StrCheckR = new string[3];
				if (num == -8)
				{
					num = 8;
				}
			}
			StrCheck[0] = All.A.OrgName;
			StrCheckR[0] = "";
			StrCheck[1] = All.A.PointName;
			StrCheckR[1] = "";
			StrCheck[2] = All.A.PointAddr;
			StrCheckR[2] = "";
			if (All.A.INN.Trim().Length > 1)
			{
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПН " + All.A.INN;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			if (All.A.TIN.Trim().Length > 1)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ІД " + All.A.TIN;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			string onOf = "офлайн";
			if (ExportXML)
			{
				onOf = ((!All.l.OfflineTrue()) ? "онлайн" : "офлайн");
			}
			else if (Operators.CompareString(typPrintChecks.ReturnOffline, "0", false) == 0)
			{
				onOf = "онлайн";
			}
			switch (num)
			{
			case 0:
				XMLtoDim(typPrintChecks.ReturnStr, vosvrat: false, onOf, typPrintChecks.ReturnMac);
				break;
			case 1:
				XMLtoDim(typPrintChecks.ReturnStr, vosvrat: true, onOf, typPrintChecks.ReturnMac);
				break;
			case 2:
				XMLtoDimS(typPrintChecks.ReturnStr, onOf);
				break;
			case 3:
				XMLtoDimZ(typPrintChecks.ReturnStr, onOf);
				break;
			case 4:
				XMLtoDimX(CheckXML);
				break;
			case 5:
				XMLtoDimPeriod(CheckXML);
				break;
			case 8:
				XMLtoDimEPZ(typPrintChecks.ReturnStr, onOf, typPrintChecks.ReturnMac);
				break;
			default:
				XMLtoAll(typPrintChecks.ReturnStr, onOf);
				break;
			}
		}
	}

	private void TransferData()
	{
		StrCheckN = new string[1];
		checked
		{
			int num = StrCheck.Count() - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(StrCheckR[i].Trim(), "", false) == 0)
				{
					StrokaCen(StrCheck[i], Dlstr);
				}
				else if (Operators.CompareString(StrCheckR[i], "---", false) == 0)
				{
					StrokaRazdela(Dlstr);
				}
				else
				{
					StrokaAll(StrCheck[i], StrCheckR[i], Dlstr);
				}
			}
		}
	}

	private string StrokaAll(string s1, string s2, int dl)
	{
		if (Operators.CompareString(s1, "", false) == 0)
		{
			return s1;
		}
		int length = s1.Length;
		int length2 = s2.Length;
		checked
		{
			string text;
			if (length + length2 > dl)
			{
				if (s1.Length > dl)
				{
					text = s1.Substring(0, dl);
				}
				else
				{
					text = s1 + Strings.Space(dl - length + 1);
					s1 = text;
				}
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN = ref StrCheckN;
				strCheckN = (string[])Utils.CopyArray((Array)strCheckN, (Array)new string[StrCheckN.Count() + 1]);
				int num = s1.Length - dl;
				return StrokaAll(s1.Substring(s1.Length - num), s2, dl);
			}
			if (length + length2 < dl)
			{
				int num2 = dl - (length + length2);
				text = s1 + Strings.Space(num2) + s2;
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN2 = ref StrCheckN;
				strCheckN2 = (string[])Utils.CopyArray((Array)strCheckN2, (Array)new string[StrCheckN.Count() + 1]);
				return "";
			}
			text = s1 + s2;
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN3 = ref StrCheckN;
			strCheckN3 = (string[])Utils.CopyArray((Array)strCheckN3, (Array)new string[StrCheckN.Count() + 1]);
			return "";
		}
	}

	private string StrokaCen(string s, int dl)
	{
		if (Operators.CompareString(s, "", false) == 0)
		{
			return s;
		}
		checked
		{
			string text;
			if (s.Length > dl)
			{
				text = s.Substring(0, dl);
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN = ref StrCheckN;
				strCheckN = (string[])Utils.CopyArray((Array)strCheckN, (Array)new string[StrCheckN.Count() + 1]);
				int num = s.Length - dl;
				return StrokaCen(s.Substring(s.Length - num), dl);
			}
			if (s.Length < dl)
			{
				int num2 = dl - s.Length;
				int num3 = unchecked(num2 / 2);
				int num4 = num2 - num3;
				text = Strings.Space(num3) + s + Strings.Space(num4);
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN2 = ref StrCheckN;
				strCheckN2 = (string[])Utils.CopyArray((Array)strCheckN2, (Array)new string[StrCheckN.Count() + 1]);
				return "";
			}
			text = s;
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN3 = ref StrCheckN;
			strCheckN3 = (string[])Utils.CopyArray((Array)strCheckN3, (Array)new string[StrCheckN.Count() + 1]);
			return "";
		}
	}

	private string StrokaRazdela(int dl, string raz = "-")
	{
		string text = "";
		checked
		{
			for (int i = 1; i <= dl; i++)
			{
				text += raz;
			}
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN = ref StrCheckN;
			strCheckN = (string[])Utils.CopyArray((Array)strCheckN, (Array)new string[StrCheckN.Count() + 1]);
			return text;
		}
	}

	private string LongToData(string LongDT, bool ForLink = false)
	{
		if (LongDT.Length != 14)
		{
			return "дата";
		}
		if (!ForLink)
		{
			return Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]) + "." + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + "." + Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]);
		}
		return Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]) + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]);
	}

	private string TimeToTimeWWW(string TimeCheck)
	{
		return Conversions.ToString(TimeCheck[0]) + Conversions.ToString(TimeCheck[1]) + Conversions.ToString(TimeCheck[3]) + Conversions.ToString(TimeCheck[4]);
	}

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}

	private int TypChekcs(string xmlCheck)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(xmlCheck, "t", "rq/dat/c");
		if (parametrToString.errCode == 0)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, "0", false) == 0)
			{
				return 0;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "1", false) == 0)
			{
				return 1;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "2", false) == 0)
			{
				return 2;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "3", false) == 0)
			{
				return 3;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "8", false) == 0)
			{
				return -8;
			}
		}
		if (All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").errCode == 0)
		{
			return 3;
		}
		return -1;
	}

	private void XMLtoDim(string xmlCheck, bool vosvrat = false, string OnOf = "онлайн", string MACcur = "МакМакМак")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string[] array = new string[101];
			int num = 0;
			do
			{
				array[num] = "";
				num++;
			}
			while (num <= 100);
			if (xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@email") != null)
			{
				array[0] = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@email").Value;
			}
			else
			{
				array[0] = "0";
			}
			string text = "";
			text = ((xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@taxa") == null) ? "" : xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@taxa").Value);
			bool flag = false;
			bool flag2 = false;
			TypDopTeg typDopTeg = default(TypDopTeg);
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pa") != null)
			{
				typDopTeg.PA = xmlDocument.SelectSingleNode("rq/dat/c/e/@pa").Value;
			}
			else
			{
				typDopTeg.PA = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pb") != null)
			{
				typDopTeg.PB = xmlDocument.SelectSingleNode("rq/dat/c/e/@pb").Value;
			}
			else
			{
				typDopTeg.PB = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pc") != null)
			{
				typDopTeg.PC = xmlDocument.SelectSingleNode("rq/dat/c/e/@pc").Value;
			}
			else
			{
				typDopTeg.PC = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pd") != null)
			{
				typDopTeg.PD = xmlDocument.SelectSingleNode("rq/dat/c/e/@pd").Value;
			}
			else
			{
				typDopTeg.PD = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pe") != null)
			{
				typDopTeg.PE = xmlDocument.SelectSingleNode("rq/dat/c/e/@pe").Value;
			}
			else
			{
				typDopTeg.PE = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@psnm") != null)
			{
				typDopTeg.PSNM = xmlDocument.SelectSingleNode("rq/dat/c/e/@psnm").Value;
			}
			else
			{
				typDopTeg.PSNM = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@rrn") != null)
			{
				typDopTeg.RRN = xmlDocument.SelectSingleNode("rq/dat/c/e/@rrn").Value;
			}
			else
			{
				typDopTeg.RRN = "";
			}
			if (xmlDocument.SelectSingleNode("rq/dat/c/e/@pf") != null)
			{
				typDopTeg.PF = xmlDocument.SelectSingleNode("rq/dat/c/e/@pf").Value;
			}
			else
			{
				typDopTeg.PF = "";
			}
			num = 1;
			do
			{
				if (!flag)
				{
					string xpath = "rq/dat/c/webcheck/@up" + num;
					if (xmlDocument.SelectSingleNode(xpath) != null)
					{
						string value = xmlDocument.SelectSingleNode(xpath).Value;
						array[num] = value;
						if (num == 1)
						{
							ref string[] strCheck2 = ref StrCheck;
							strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
							ref string[] strCheckR2 = ref StrCheckR;
							strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = "";
							StrCheckR[StrCheck.Count() - 1] = "---";
						}
						ref string[] strCheck3 = ref StrCheck;
						strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR3 = ref StrCheckR;
						strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = array[num];
						StrCheckR[StrCheck.Count() - 1] = "#";
					}
					else
					{
						array[num] = "";
						flag = true;
					}
				}
				if (!flag2)
				{
					string xpath2 = "rq/dat/c/webcheck/@dn" + num;
					if (xmlDocument.SelectSingleNode(xpath2) != null)
					{
						string value2 = xmlDocument.SelectSingleNode(xpath2).Value;
						array[num + 50] = value2;
					}
					else
					{
						array[num + 50] = "";
						flag2 = true;
					}
				}
				if (unchecked(flag && flag2))
				{
					break;
				}
				num++;
			}
			while (num <= 50);
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("p");
			int num2 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num3 = num2;
			for (int i = 0; i <= num3; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr = All.d.GetParametrToString(outerXml, "q", "p").ReturnStr;
				returnStr = (All.A.PointRegion ? Strings.Replace(returnStr, ",", ".", 1, -1, (CompareMethod)0) : Strings.Replace(returnStr, ".", ",", 1, -1, (CompareMethod)0));
				double num4 = 0.0;
				double num5 = 0.0;
				double num6 = 0.0;
				string text2 = "";
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = All.KolvoVes(returnStr) + " x";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				double num7 = All.StrToDouble(returnStr);
				num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				num5 = num7 * num4;
				string returnStr2 = All.d.GetParametrToString(outerXml, "cd", "p").ReturnStr;
				TypProductName typProductName = All.DecoderProductName(All.d.GetParametrToString(outerXml, "nm", "p", RegUpLow: true).ReturnStr);
				if (typProductName.Uktzed.Length > 0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typProductName.Uktzed;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (returnStr2.Length > 0)
				{
					ref string[] strCheck7 = ref StrCheck;
					strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR7 = ref StrCheckR;
					strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = returnStr2;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typProductName.Excisestamp.Length > 0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typProductName.Excisestamp;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				ref string[] strCheck9 = ref StrCheck;
				strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR9 = ref StrCheckR;
				strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = typProductName.Name;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num5.ToString());
				string returnStr3 = All.d.GetParametrToString(outerXml, "tx", "p").ReturnStr;
				returnStr3 = All.PayTax.NUMtoABC(returnStr3);
				StrCheckR[StrCheck.Count() - 1] = StrCheckR[StrCheck.Count() - 1] + " " + returnStr3;
				double num8 = All.StrToDouble(All.d.GetParametrToString(outerXml, "sm", "p").ReturnStr);
				num5 = All.StrToDouble(All.Bablo(num5.ToString()));
				num6 = All.StrToDouble(All.Bablo(num8 - num5));
				double num9 = All.StrToDouble(All.d.GetParametrToString(outerXml, "avans", "p").ReturnStr);
				string returnStr4 = All.d.GetParametrToString(outerXml, "avansm", "p").ReturnStr;
				num6 += num9;
				string text3 = "";
				if (num6 > 0.0)
				{
					text2 = "НАЦIНКА";
					text3 = "";
				}
				else if (num6 < 0.0)
				{
					text2 = "ЗНИЖКА";
					text3 = "-";
				}
				if (Operators.CompareString(text2, "", false) != 0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = text2;
					StrCheckR[StrCheck.Count() - 1] = text3 + All.Bablo(Math.Abs(num6)) + " " + returnStr3;
				}
				if (num9 > 0.0)
				{
					ref string[] strCheck11 = ref StrCheck;
					strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR11 = ref StrCheckR;
					strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = returnStr4.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = "-" + All.Bablo(num9);
				}
			}
			bool flag3 = false;
			if (All.A.Showacquiring)
			{
				if (typDopTeg.PA.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PA;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PB.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck13 = ref StrCheck;
					strCheck13 = (string[])Utils.CopyArray((Array)strCheck13, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR13 = ref StrCheckR;
					strCheckR13 = (string[])Utils.CopyArray((Array)strCheckR13, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ТЕРМIНАЛ: " + typDopTeg.PB;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PF.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray((Array)strCheck14, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray((Array)strCheckR14, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОМІСІЯ: " + typDopTeg.PF + " грн";
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PC.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck15 = ref StrCheck;
					strCheck15 = (string[])Utils.CopyArray((Array)strCheck15, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR15 = ref StrCheckR;
					strCheckR15 = (string[])Utils.CopyArray((Array)strCheckR15, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PC;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PD.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck16 = ref StrCheck;
					strCheck16 = (string[])Utils.CopyArray((Array)strCheck16, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR16 = ref StrCheckR;
					strCheckR16 = (string[])Utils.CopyArray((Array)strCheckR16, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ЕПЗ: " + typDopTeg.PD;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PSNM.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck17 = ref StrCheck;
					strCheck17 = (string[])Utils.CopyArray((Array)strCheck17, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR17 = ref StrCheckR;
					strCheckR17 = (string[])Utils.CopyArray((Array)strCheckR17, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ПЛАТIЖНА СИСТЕМА:" + typDopTeg.PSNM;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PE.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck18 = ref StrCheck;
					strCheck18 = (string[])Utils.CopyArray((Array)strCheck18, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR18 = ref StrCheckR;
					strCheckR18 = (string[])Utils.CopyArray((Array)strCheckR18, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД АВТОРИЗАЦІЇ:" + typDopTeg.PE;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.RRN.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck19 = ref StrCheck;
					strCheck19 = (string[])Utils.CopyArray((Array)strCheck19, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR19 = ref StrCheckR;
					strCheckR19 = (string[])Utils.CopyArray((Array)strCheckR19, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД ТРАНЗ.:" + typDopTeg.RRN;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
			}
			if (array[51].Trim().Length > 0 && !flag3)
			{
				flag3 = DrawRazdel();
			}
			num = 51;
			do
			{
				if (array[num].Trim().Length > 0)
				{
					ref string[] strCheck20 = ref StrCheck;
					strCheck20 = (string[])Utils.CopyArray((Array)strCheck20, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR20 = ref StrCheckR;
					strCheckR20 = (string[])Utils.CopyArray((Array)strCheckR20, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[num];
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				num++;
			}
			while (num <= 100);
			ref string[] strCheck21 = ref StrCheck;
			strCheck21 = (string[])Utils.CopyArray((Array)strCheck21, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR21 = ref StrCheckR;
			strCheckR21 = (string[])Utils.CopyArray((Array)strCheckR21, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num2 = elementsByTagName.Count - 1;
			string text4 = "";
			string[,] array2 = new string[num2 + 1, 4];
			double num10 = 0.0;
			double num11 = 0.0;
			double num12 = 0.0;
			int num13 = num2;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr5 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr5, "", false) != 0)
				{
					array2[i, 0] = returnStr5.ToUpper();
					array2[i, 1] = All.d.GetParametrToString(outerXml, "sm", "m").ReturnStr;
					array2[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array2[i, 3] = " ";
					if (!Versioned.IsNumeric((object)array2[i, 2]))
					{
						array2[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array2[i, 2]) == 2) & (Operators.CompareString(array2[i, 0], "КАРТКА", false) == 0))
					{
						array2[i, 2] = "3";
					}
					if (Conversions.ToInteger(array2[i, 2]) > 2)
					{
						array2[i, 2] = "1";
					}
					if (Operators.CompareString(array2[i, 2], "0", false) == 0)
					{
						num10 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(array2[i, 2], "1", false) == 0)
					{
						num11 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(array2[i, 2], "2", false) == 0)
					{
						num12 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(returnStr5.ToLower(), "готівка", false) == 0 && Operators.CompareString(All.d.GetParametrToString(outerXml, "rm", "m").ReturnStr, "", false) != 0)
					{
						text4 = All.Bablo(All.d.GetParametrToString(outerXml, "rm", "m").ReturnStr);
					}
				}
			}
			string text5 = All.f.GetString("Global", "CheckPayForms");
			if (Operators.CompareString(text5, "", false) == 0)
			{
				text5 = "2";
				All.f.WriteString("Global", "CheckPayForms", text5);
			}
			if (num10 > 0.0)
			{
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray((Array)strCheck22, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray((Array)strCheckR22, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num10) + " грн";
			}
			if (num11 > 0.0)
			{
				if (Operators.CompareString(text5, "2", false) != 0)
				{
					ref string[] strCheck23 = ref StrCheck;
					strCheck23 = (string[])Utils.CopyArray((Array)strCheck23, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR23 = ref StrCheckR;
					strCheckR23 = (string[])Utils.CopyArray((Array)strCheckR23, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num11) + " грн";
				}
				if (!vosvrat)
				{
					int num14 = num2;
					for (int i = 0; i <= num14; i++)
					{
						if (((Conversions.ToInteger(array2[i, 2]) == 1) | (Conversions.ToInteger(array2[i, 2]) > 2)) && All.StrToDouble(array2[i, 1]) > 0.0)
						{
							if (Operators.CompareString(text5, "2", false) == 0)
							{
								ref string[] strCheck24 = ref StrCheck;
								strCheck24 = (string[])Utils.CopyArray((Array)strCheck24, (Array)new string[StrCheck.Count() + 1]);
								ref string[] strCheckR24 = ref StrCheckR;
								strCheckR24 = (string[])Utils.CopyArray((Array)strCheckR24, (Array)new string[StrCheck.Count() + 1]);
								StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]) + " грн";
							}
							ref string[] strCheck25 = ref StrCheck;
							strCheck25 = (string[])Utils.CopyArray((Array)strCheck25, (Array)new string[StrCheck.Count() + 1]);
							ref string[] strCheckR25 = ref StrCheckR;
							strCheckR25 = (string[])Utils.CopyArray((Array)strCheckR25, (Array)new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = array2[i, 3] + array2[i, 0];
							if (Operators.CompareString(text5, "1", false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]);
							}
							else
							{
								StrCheckR[StrCheck.Count() - 1] = "#";
							}
						}
					}
				}
				else if (Operators.CompareString(text5, "2", false) == 0)
				{
					ref string[] strCheck26 = ref StrCheck;
					strCheck26 = (string[])Utils.CopyArray((Array)strCheck26, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR26 = ref StrCheckR;
					strCheckR26 = (string[])Utils.CopyArray((Array)strCheckR26, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num11) + " грн";
				}
			}
			if (num12 > 0.0)
			{
				if (Operators.CompareString(text5, "2", false) != 0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray((Array)strCheck27, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray((Array)strCheckR27, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num12) + " грн";
				}
				if (!vosvrat)
				{
					int num15 = num2;
					for (int i = 0; i <= num15; i++)
					{
						if (Conversions.ToInteger(array2[i, 2]) == 2 && All.StrToDouble(array2[i, 1]) > 0.0)
						{
							if (Operators.CompareString(text5, "2", false) == 0)
							{
								ref string[] strCheck28 = ref StrCheck;
								strCheck28 = (string[])Utils.CopyArray((Array)strCheck28, (Array)new string[StrCheck.Count() + 1]);
								ref string[] strCheckR28 = ref StrCheckR;
								strCheckR28 = (string[])Utils.CopyArray((Array)strCheckR28, (Array)new string[StrCheck.Count() + 1]);
								StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]) + " грн";
							}
							ref string[] strCheck29 = ref StrCheck;
							strCheck29 = (string[])Utils.CopyArray((Array)strCheck29, (Array)new string[StrCheck.Count() + 1]);
							ref string[] strCheckR29 = ref StrCheckR;
							strCheckR29 = (string[])Utils.CopyArray((Array)strCheckR29, (Array)new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = array2[i, 3] + array2[i, 0];
							if (Operators.CompareString(text5, "1", false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]);
							}
							else
							{
								StrCheckR[StrCheck.Count() - 1] = "#";
							}
						}
					}
				}
				else if (Operators.CompareString(text5, "2", false) == 0)
				{
					ref string[] strCheck30 = ref StrCheck;
					strCheck30 = (string[])Utils.CopyArray((Array)strCheck30, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR30 = ref StrCheckR;
					strCheckR30 = (string[])Utils.CopyArray((Array)strCheckR30, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num12) + " грн";
				}
			}
			ref string[] strCheck31 = ref StrCheck;
			strCheck31 = (string[])Utils.CopyArray((Array)strCheck31, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR31 = ref StrCheckR;
			strCheckR31 = (string[])Utils.CopyArray((Array)strCheckR31, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr6 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/e").ReturnStr;
			if (Operators.CompareString(returnStr6, "", false) != 0)
			{
				ref string[] strCheck32 = ref StrCheck;
				strCheck32 = (string[])Utils.CopyArray((Array)strCheck32, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR32 = ref StrCheckR;
				strCheckR32 = (string[])Utils.CopyArray((Array)strCheckR32, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СУМА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr6) + " грн";
			}
			elementsByTagName = xmlDocument.GetElementsByTagName("tx");
			num2 = elementsByTagName.Count - 1;
			double num16 = 0.0;
			string text6 = "";
			bool flag4 = false;
			int num17 = num2;
			for (int i = 0; i <= num17; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr7 = All.d.GetParametrToString(outerXml, "tx", "tx").ReturnStr;
				if (Operators.CompareString(returnStr7, "", false) != 0)
				{
					if ((Conversions.ToInteger(returnStr7) < 4) | (Conversions.ToInteger(returnStr7) > 7))
					{
						if (!unchecked(Operators.CompareString(returnStr7, "1", false) == 0 && flag4))
						{
							ref string[] strCheck33 = ref StrCheck;
							strCheck33 = (string[])Utils.CopyArray((Array)strCheck33, (Array)new string[StrCheck.Count() + 1]);
							ref string[] strCheckR33 = ref StrCheckR;
							strCheckR33 = (string[])Utils.CopyArray((Array)strCheckR33, (Array)new string[StrCheck.Count() + 1]);
						}
						switch (returnStr7)
						{
						case "8":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=НЕОПОД.";
							break;
						case "9":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=БЕЗ ПДВ";
							break;
						case "10":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=НЕ ОПОДАТКОВУЄТЬСЯ";
							break;
						default:
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=" + All.PayTax.get_TaxPRC(Conversions.ToInteger(returnStr7)) + "%";
							break;
						}
						if (Operators.CompareString(returnStr7, "1", false) == 0)
						{
							if (Operators.CompareString(text.Trim(), "", false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txsm", "tx").ReturnStr);
							}
							else if (!flag4)
							{
								flag4 = true;
								StrCheckR[StrCheck.Count() - 1] = text;
							}
						}
						else
						{
							StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txsm", "tx").ReturnStr);
						}
					}
					else if (Operators.CompareString(text.Trim(), "", false) != 0 && ((Operators.CompareString(returnStr7, "4", false) == 0) | (Operators.CompareString(returnStr7, "6", false) == 0)) && !flag4)
					{
						flag4 = true;
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray((Array)strCheck34, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray((Array)strCheckR34, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC("1") + "=" + All.PayTax.get_TaxPRC(1) + "%";
						StrCheckR[StrCheck.Count() - 1] = text;
					}
				}
				string returnStr8 = All.d.GetParametrToString(outerXml, "dtpr", "tx").ReturnStr;
				if (All.StrToDouble(returnStr8) > 0.0)
				{
					string returnStr9 = All.d.GetParametrToString(outerXml, "dtsm", "tx").ReturnStr;
					num16 += All.StrToDouble(returnStr9);
					if (All.StrToDouble(returnStr8) == 7.5)
					{
						text6 = "ПФ  Д=7.5%";
					}
					else if (All.StrToDouble(returnStr8) == 5.0)
					{
						text6 = "АКЦ.ПОД. Г=5%";
					}
				}
			}
			if (Operators.CompareString(text6, "", false) != 0)
			{
				ref string[] strCheck35 = ref StrCheck;
				strCheck35 = (string[])Utils.CopyArray((Array)strCheck35, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR35 = ref StrCheckR;
				strCheckR35 = (string[])Utils.CopyArray((Array)strCheckR35, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text6;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num16);
			}
			string returnStr10 = All.d.GetParametrToString(xmlCheck, "smp", "rq/dat/c/m").ReturnStr;
			string returnStr11 = All.d.GetParametrToString(xmlCheck, "smm", "rq/dat/c/m").ReturnStr;
			string text7 = returnStr6;
			if (returnStr10.Length > 0)
			{
				ref string[] strCheck36 = ref StrCheck;
				strCheck36 = (string[])Utils.CopyArray((Array)strCheck36, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR36 = ref StrCheckR;
				strCheckR36 = (string[])Utils.CopyArray((Array)strCheckR36, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОКРУГЛЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr10);
				text7 = All.Bablo(All.StrToDouble(returnStr6) + All.StrToDouble(returnStr10));
			}
			else if (returnStr11.Length > 0)
			{
				ref string[] strCheck37 = ref StrCheck;
				strCheck37 = (string[])Utils.CopyArray((Array)strCheck37, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR37 = ref StrCheckR;
				strCheckR37 = (string[])Utils.CopyArray((Array)strCheckR37, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОКРУГЛЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr11);
				text7 = All.Bablo(All.StrToDouble(returnStr6) - All.StrToDouble(returnStr11));
			}
			if (!vosvrat)
			{
				ref string[] strCheck38 = ref StrCheck;
				strCheck38 = (string[])Utils.CopyArray((Array)strCheck38, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR38 = ref StrCheckR;
				strCheckR38 = (string[])Utils.CopyArray((Array)strCheckR38, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ДО СПЛАТИ";
				StrCheckR[StrCheck.Count() - 1] = text7 + " грн";
			}
			if (Operators.CompareString(text4, "", false) != 0)
			{
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray((Array)strCheck39, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray((Array)strCheckR39, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "РЕШТА";
				StrCheckR[StrCheck.Count() - 1] = text4 + " грн";
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray((Array)strCheck40, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray((Array)strCheckR40, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = "ЧЕК № " + Tb2;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray((Array)strCheck41, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray((Array)strCheckR41, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			Secondary.SendMail[0] = array[0];
			Secondary.SendMail[2] = Tb2;
			Secondary.SendMail[3] = DataWWW + "&time=" + TimeWWW;
			Secondary.SendMail[5] = LongToData(innerText);
			DataTimePr = StrCheck[StrCheck.Count() - 1] + StrCheckR[StrCheck.Count() - 1];
			MacPr = MACcur;
			FiChPr = Tb2;
			SumPr = All.Bablo(returnStr6);
			FnPr = All.A.FN;
			if (!ExportXML)
			{
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray((Array)strCheck42, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray((Array)strCheckR42, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "HotGamesBest";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck43 = ref StrCheck;
			strCheck43 = (string[])Utils.CopyArray((Array)strCheck43, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR43 = ref StrCheckR;
			strCheckR43 = (string[])Utils.CopyArray((Array)strCheckR43, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			if (Operators.CompareString(OnOf, "офлайн", false) == 0)
			{
				ref string[] strCheck44 = ref StrCheck;
				strCheck44 = (string[])Utils.CopyArray((Array)strCheck44, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR44 = ref StrCheckR;
				strCheckR44 = (string[])Utils.CopyArray((Array)strCheckR44, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = MACcur;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray((Array)strCheck45, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray((Array)strCheckR45, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			if (vosvrat)
			{
				ref string[] strCheck46 = ref StrCheck;
				strCheck46 = (string[])Utils.CopyArray((Array)strCheck46, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR46 = ref StrCheckR;
				strCheckR46 = (string[])Utils.CopyArray((Array)strCheckR46, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ВИДАТКОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", false) == 0))
			{
				ref string[] strCheck47 = ref StrCheck;
				strCheck47 = (string[])Utils.CopyArray((Array)strCheck47, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR47 = ref StrCheckR;
				strCheckR47 = (string[])Utils.CopyArray((Array)strCheckR47, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else if (!vosvrat)
			{
				ref string[] strCheck48 = ref StrCheck;
				strCheck48 = (string[])Utils.CopyArray((Array)strCheck48, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR48 = ref StrCheckR;
				strCheckR48 = (string[])Utils.CopyArray((Array)strCheckR48, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private string Okruglit(string m)
	{
		return All.Bablo(Strings.FormatNumber((object)All.StrToDouble(m), 1, (TriState)(-2), (TriState)(-2), (TriState)(-2)));
	}

	private void XMLtoDimEPZ(string xmlCheck, string OnOf = "онлайн", string MACcur = "МакМакМак")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string[] array = new string[101];
			int num = 0;
			do
			{
				array[num] = "";
				num++;
			}
			while (num <= 100);
			try
			{
				array[0] = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@email").Value + "'";
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				array[0] = "0";
				ProjectData.ClearProjectError();
			}
			try
			{
				_ = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@taxa").Value;
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				ProjectData.ClearProjectError();
			}
			bool flag = false;
			bool flag2 = false;
			TypDopTeg typDopTeg = default(TypDopTeg);
			try
			{
				typDopTeg.PA = xmlDocument.SelectSingleNode("rq/dat/c/e/@pa").Value;
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				typDopTeg.PA = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PB = xmlDocument.SelectSingleNode("rq/dat/c/e/@pb").Value;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				typDopTeg.PB = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PC = xmlDocument.SelectSingleNode("rq/dat/c/e/@pc").Value;
			}
			catch (Exception ex11)
			{
				ProjectData.SetProjectError(ex11);
				Exception ex12 = ex11;
				typDopTeg.PC = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PD = xmlDocument.SelectSingleNode("rq/dat/c/e/@pd").Value;
			}
			catch (Exception ex13)
			{
				ProjectData.SetProjectError(ex13);
				Exception ex14 = ex13;
				typDopTeg.PD = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PE = xmlDocument.SelectSingleNode("rq/dat/c/e/@pe").Value;
			}
			catch (Exception ex15)
			{
				ProjectData.SetProjectError(ex15);
				Exception ex16 = ex15;
				typDopTeg.PE = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PSNM = xmlDocument.SelectSingleNode("rq/dat/c/e/@psnm").Value;
			}
			catch (Exception ex17)
			{
				ProjectData.SetProjectError(ex17);
				Exception ex18 = ex17;
				typDopTeg.PSNM = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.RRN = xmlDocument.SelectSingleNode("rq/dat/c/e/@rrn").Value;
			}
			catch (Exception ex19)
			{
				ProjectData.SetProjectError(ex19);
				Exception ex20 = ex19;
				typDopTeg.RRN = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PF = xmlDocument.SelectSingleNode("rq/dat/c/e/@pf").Value;
			}
			catch (Exception ex21)
			{
				ProjectData.SetProjectError(ex21);
				Exception ex22 = ex21;
				typDopTeg.PF = "";
				ProjectData.ClearProjectError();
			}
			num = 1;
			do
			{
				if (!flag)
				{
					string xpath = "rq/dat/c/webcheck/@up" + num;
					try
					{
						array[num] = xmlDocument.SelectSingleNode(xpath).Value;
						if (num == 1)
						{
							ref string[] strCheck2 = ref StrCheck;
							strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
							ref string[] strCheckR2 = ref StrCheckR;
							strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = "";
							StrCheckR[StrCheck.Count() - 1] = "---";
						}
						ref string[] strCheck3 = ref StrCheck;
						strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR3 = ref StrCheckR;
						strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = array[num];
						StrCheckR[StrCheck.Count() - 1] = "#";
					}
					catch (Exception ex23)
					{
						ProjectData.SetProjectError(ex23);
						Exception ex24 = ex23;
						array[num] = "";
						flag = true;
						ProjectData.ClearProjectError();
					}
				}
				if (!flag2)
				{
					string xpath2 = "rq/dat/c/webcheck/@dn" + num;
					try
					{
						array[num + 50] = xmlDocument.SelectSingleNode(xpath2).Value;
					}
					catch (Exception ex25)
					{
						ProjectData.SetProjectError(ex25);
						Exception ex26 = ex25;
						array[num + 50] = "";
						flag2 = true;
						ProjectData.ClearProjectError();
					}
				}
				if (unchecked(flag && flag2))
				{
					break;
				}
				num++;
			}
			while (num <= 50);
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("p");
			int num2 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num3 = num2;
			for (int i = 0; i <= num3; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr = All.d.GetParametrToString(outerXml, "q", "p").ReturnStr;
				returnStr = (All.A.PointRegion ? Strings.Replace(returnStr, ",", ".", 1, -1, (CompareMethod)0) : Strings.Replace(returnStr, ".", ",", 1, -1, (CompareMethod)0));
				double num4 = 0.0;
				double num5 = 0.0;
				double num6 = All.StrToDouble(returnStr);
				num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				num5 = num6 * num4;
				All.d.GetParametrToString(outerXml, "cd", "p");
				All.DecoderProductName(All.d.GetParametrToString(outerXml, "nm", "p", RegUpLow: true).ReturnStr);
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЯМ ЕПЗ   ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num5.ToString()) + " ГРН";
			}
			bool flag3 = false;
			if (All.A.Showacquiring)
			{
				if (typDopTeg.PA.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PA;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PB.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck7 = ref StrCheck;
					strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR7 = ref StrCheckR;
					strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ТЕРМIНАЛ: " + typDopTeg.PB;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PF.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОМІСІЯ: " + typDopTeg.PF + " грн";
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PC.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck9 = ref StrCheck;
					strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR9 = ref StrCheckR;
					strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PC;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PD.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ЕПЗ: " + typDopTeg.PD;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PSNM.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck11 = ref StrCheck;
					strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR11 = ref StrCheckR;
					strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ПЛАТIЖНА СИСТЕМА:" + typDopTeg.PSNM;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PE.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД АВТОРИЗАЦІЇ:" + typDopTeg.PE;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.RRN.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck13 = ref StrCheck;
					strCheck13 = (string[])Utils.CopyArray((Array)strCheck13, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR13 = ref StrCheckR;
					strCheckR13 = (string[])Utils.CopyArray((Array)strCheckR13, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД ТРАНЗ.:" + typDopTeg.RRN;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
			}
			if (array[51].Trim().Length > 0 && !flag3)
			{
				flag3 = DrawRazdel();
			}
			num = 51;
			do
			{
				if (array[num].Trim().Length > 0)
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray((Array)strCheck14, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray((Array)strCheckR14, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[num];
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				num++;
			}
			while (num <= 100);
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray((Array)strCheck15, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray((Array)strCheckR15, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/e").ReturnStr;
			elementsByTagName = xmlDocument.GetElementsByTagName("tx");
			_ = elementsByTagName.Count - 1;
			ref string[] strCheck16 = ref StrCheck;
			strCheck16 = (string[])Utils.CopyArray((Array)strCheck16, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR16 = ref StrCheckR;
			strCheckR16 = (string[])Utils.CopyArray((Array)strCheckR16, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = Tb2;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck17 = ref StrCheck;
			strCheck17 = (string[])Utils.CopyArray((Array)strCheck17, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR17 = ref StrCheckR;
			strCheckR17 = (string[])Utils.CopyArray((Array)strCheckR17, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			DataTimePr = StrCheck[StrCheck.Count() - 1] + StrCheckR[StrCheck.Count() - 1];
			MacPr = MACcur;
			FiChPr = Tb2;
			SumPr = All.Bablo(returnStr2);
			FnPr = All.A.FN;
			ref string[] strCheck18 = ref StrCheck;
			strCheck18 = (string[])Utils.CopyArray((Array)strCheck18, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR18 = ref StrCheckR;
			strCheckR18 = (string[])Utils.CopyArray((Array)strCheckR18, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "HotGamesBest";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray((Array)strCheck19, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray((Array)strCheckR19, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			if (Operators.CompareString(OnOf, "офлайн", false) == 0)
			{
				ref string[] strCheck20 = ref StrCheck;
				strCheck20 = (string[])Utils.CopyArray((Array)strCheck20, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR20 = ref StrCheckR;
				strCheckR20 = (string[])Utils.CopyArray((Array)strCheckR20, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = MACcur;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck21 = ref StrCheck;
			strCheck21 = (string[])Utils.CopyArray((Array)strCheck21, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR21 = ref StrCheckR;
			strCheckR21 = (string[])Utils.CopyArray((Array)strCheckR21, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", false) == 0))
			{
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray((Array)strCheck22, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray((Array)strCheckR22, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else
			{
				ref string[] strCheck23 = ref StrCheck;
				strCheck23 = (string[])Utils.CopyArray((Array)strCheck23, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR23 = ref StrCheckR;
				strCheckR23 = (string[])Utils.CopyArray((Array)strCheckR23, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЧЕК ВИДАЧІ КОШТІВ";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private bool DrawRazdel()
	{
		ref string[] strCheck = ref StrCheck;
		checked
		{
			strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR = ref StrCheckR;
			strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			return true;
		}
	}

	private void XMLtoDimS(string xmlCheck, string OnOf = "онлайн")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/i").ReturnStr;
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/o").ReturnStr;
			if (Operators.CompareString(returnStr.Trim(), "", false) == 0)
			{
				returnStr = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/c/i").ReturnStr;
			}
			if (Operators.CompareString(returnStr2.Trim(), "", false) == 0)
			{
				returnStr2 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/c/o").ReturnStr;
			}
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			if (Operators.CompareString(returnStr, "", false) != 0)
			{
				ref string[] strCheck3 = ref StrCheck;
				strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR3 = ref StrCheckR;
				strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr);
			}
			else if (Operators.CompareString(returnStr2, "", false) != 0)
			{
				ref string[] strCheck4 = ref StrCheck;
				strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR4 = ref StrCheckR;
				strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr2);
			}
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = Tb2;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "С Л У Ж Б О В И Й   Ч Е К";
			StrCheckR[StrCheck.Count() - 1] = "";
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", false) == 0))
			{
				ref string[] strCheck10 = ref StrCheck;
				strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR10 = ref StrCheckR;
				strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else
			{
				ref string[] strCheck11 = ref StrCheck;
				strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR11 = ref StrCheckR;
				strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private void XMLtoDimZ(string xmlCheck, string OnOf = "онлайн")
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "Z ЗВIТ #" + returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string text3 = All.d.GetParametrToString(xmlCheck, "ni", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			bool flag = false;
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr2 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr2, "", false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr2.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric((object)array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr2.ToLower(), "готівка", false) == 0)
				{
					num = num6;
					flag = true;
					text4 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text4, "", false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text5, "", false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text6, "", false) == 0)
					{
						flag = false;
					}
					text7 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text7, "", false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr3, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr3.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "гб", false) == 0))
				{
					string returnStr4 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr4.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr4);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck12 = ref StrCheck;
						strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR12 = ref StrCheckR;
						strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr3.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "дб", false) == 0))
				{
					string returnStr5 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr5.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr5);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck13 = ref StrCheck;
						strCheck13 = (string[])Utils.CopyArray((Array)strCheck13, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR13 = ref StrCheckR;
						strCheckR13 = (string[])Utils.CopyArray((Array)strCheckR13, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray((Array)strCheck14, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray((Array)strCheckR14, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr3.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray((Array)strCheck15, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray((Array)strCheckR15, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr6 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr6, "", false) == 0)
				{
					continue;
				}
				string returnStr7 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr6.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "гб", false) == 0))
				{
					string returnStr8;
					if (Versioned.IsNumeric((object)text2))
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "DTI", returnStr8);
					}
					else
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "TXI", returnStr8);
					}
					if (Operators.CompareString(returnStr8.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray((Array)strCheck16, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray((Array)strCheckR16, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr6.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "дб", false) == 0))
				{
					string returnStr9;
					if (Versioned.IsNumeric((object)text2))
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "DTI", returnStr9);
					}
					else
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "TXI", returnStr9);
					}
					if (Operators.CompareString(returnStr9.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray((Array)strCheck17, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray((Array)strCheckR17, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck18 = ref StrCheck;
				strCheck18 = (string[])Utils.CopyArray((Array)strCheck18, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR18 = ref StrCheckR;
				strCheckR18 = (string[])Utils.CopyArray((Array)strCheckR18, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr6.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray((Array)strCheck19, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray((Array)strCheckR19, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck20 = ref StrCheck;
			strCheck20 = (string[])Utils.CopyArray((Array)strCheck20, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR20 = ref StrCheckR;
			strCheckR20 = (string[])Utils.CopyArray((Array)strCheckR20, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck21 = ref StrCheck;
				strCheck21 = (string[])Utils.CopyArray((Array)strCheck21, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR21 = ref StrCheckR;
				strCheckR21 = (string[])Utils.CopyArray((Array)strCheckR21, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray((Array)strCheck22, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray((Array)strCheckR22, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray((Array)strCheck23, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray((Array)strCheckR23, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray((Array)strCheck24, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray((Array)strCheckR24, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			text3 = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray((Array)strCheck25, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray((Array)strCheckR25, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr10 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr10, "", false) != 0)
				{
					array[i, 0] = returnStr10.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric((object)array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr10.ToLower(), "готівка", false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray((Array)strCheck26, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray((Array)strCheckR26, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray((Array)strCheck27, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray((Array)strCheckR27, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck28 = ref StrCheck;
			strCheck28 = (string[])Utils.CopyArray((Array)strCheck28, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR28 = ref StrCheckR;
			strCheckR28 = (string[])Utils.CopyArray((Array)strCheckR28, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck29 = ref StrCheck;
					strCheck29 = (string[])Utils.CopyArray((Array)strCheck29, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR29 = ref StrCheckR;
					strCheckR29 = (string[])Utils.CopyArray((Array)strCheckR29, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck30 = ref StrCheck;
			strCheck30 = (string[])Utils.CopyArray((Array)strCheck30, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR30 = ref StrCheckR;
			strCheckR30 = (string[])Utils.CopyArray((Array)strCheckR30, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck31 = ref StrCheck;
					strCheck31 = (string[])Utils.CopyArray((Array)strCheck31, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR31 = ref StrCheckR;
					strCheckR31 = (string[])Utils.CopyArray((Array)strCheckR31, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck32 = ref StrCheck;
			strCheck32 = (string[])Utils.CopyArray((Array)strCheck32, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR32 = ref StrCheckR;
			strCheckR32 = (string[])Utils.CopyArray((Array)strCheckR32, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr11 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr11, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr11.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "гб", false) == 0))
				{
					string returnStr12 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr12.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr12);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck33 = ref StrCheck;
						strCheck33 = (string[])Utils.CopyArray((Array)strCheck33, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR33 = ref StrCheckR;
						strCheckR33 = (string[])Utils.CopyArray((Array)strCheckR33, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr11.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "дб", false) == 0))
				{
					string returnStr13 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr13.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr13);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray((Array)strCheck34, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray((Array)strCheckR34, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck35 = ref StrCheck;
					strCheck35 = (string[])Utils.CopyArray((Array)strCheck35, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR35 = ref StrCheckR;
					strCheckR35 = (string[])Utils.CopyArray((Array)strCheckR35, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr11.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck36 = ref StrCheck;
			strCheck36 = (string[])Utils.CopyArray((Array)strCheck36, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR36 = ref StrCheckR;
			strCheckR36 = (string[])Utils.CopyArray((Array)strCheckR36, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr14 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr14, "", false) == 0)
				{
					continue;
				}
				string returnStr15 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr14.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "гб", false) == 0))
				{
					string text8 = ((!Versioned.IsNumeric((object)text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text8.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(text8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck37 = ref StrCheck;
						strCheck37 = (string[])Utils.CopyArray((Array)strCheck37, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR37 = ref StrCheckR;
						strCheckR37 = (string[])Utils.CopyArray((Array)strCheckR37, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr14.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "дб", false) == 0))
				{
					string text9 = ((!Versioned.IsNumeric((object)text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text9.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(text9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck38 = ref StrCheck;
						strCheck38 = (string[])Utils.CopyArray((Array)strCheck38, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR38 = ref StrCheckR;
						strCheckR38 = (string[])Utils.CopyArray((Array)strCheckR38, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray((Array)strCheck39, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray((Array)strCheckR39, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr14.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray((Array)strCheck40, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray((Array)strCheckR40, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray((Array)strCheck41, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray((Array)strCheckR41, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray((Array)strCheck42, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray((Array)strCheckR42, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
				ref string[] strCheck43 = ref StrCheck;
				strCheck43 = (string[])Utils.CopyArray((Array)strCheck43, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR43 = ref StrCheckR;
				strCheckR43 = (string[])Utils.CopyArray((Array)strCheckR43, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text7);
			}
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray((Array)strCheck44, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray((Array)strCheckR44, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr16 = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/z/io").ReturnStr;
			string returnStr17 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/z/io").ReturnStr;
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray((Array)strCheck45, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray((Array)strCheckR45, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr16);
			num += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck46 = ref StrCheck;
			strCheck46 = (string[])Utils.CopyArray((Array)strCheck46, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR46 = ref StrCheckR;
			strCheckR46 = (string[])Utils.CopyArray((Array)strCheckR46, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
			num -= All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			string returnStr18 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr18))
			{
				num -= All.StrToDouble(returnStr18);
			}
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray((Array)strCheck47, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray((Array)strCheckR47, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА У СЕЙФІ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num.ToString());
			ref string[] strCheck48 = ref StrCheck;
			strCheck48 = (string[])Utils.CopyArray((Array)strCheck48, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR48 = ref StrCheckR;
			strCheckR48 = (string[])Utils.CopyArray((Array)strCheckR48, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr19 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr19) && Conversions.ToInteger(returnStr19) > 0)
			{
				ref string[] strCheck49 = ref StrCheck;
				strCheck49 = (string[])Utils.CopyArray((Array)strCheck49, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR49 = ref StrCheckR;
				strCheckR49 = (string[])Utils.CopyArray((Array)strCheckR49, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
				ref string[] strCheck50 = ref StrCheck;
				strCheck50 = (string[])Utils.CopyArray((Array)strCheck50, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR50 = ref StrCheckR;
				strCheckR50 = (string[])Utils.CopyArray((Array)strCheckR50, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr19;
				ref string[] strCheck51 = ref StrCheck;
				strCheck51 = (string[])Utils.CopyArray((Array)strCheck51, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR51 = ref StrCheckR;
				strCheckR51 = (string[])Utils.CopyArray((Array)strCheckR51, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			ref string[] strCheck52 = ref StrCheck;
			strCheck52 = (string[])Utils.CopyArray((Array)strCheck52, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR52 = ref StrCheckR;
			strCheckR52 = (string[])Utils.CopyArray((Array)strCheckR52, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = Tb2;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck53 = ref StrCheck;
			strCheck53 = (string[])Utils.CopyArray((Array)strCheck53, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR53 = ref StrCheckR;
			strCheckR53 = (string[])Utils.CopyArray((Array)strCheckR53, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck54 = ref StrCheck;
			strCheck54 = (string[])Utils.CopyArray((Array)strCheck54, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR54 = ref StrCheckR;
			strCheckR54 = (string[])Utils.CopyArray((Array)strCheckR54, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck55 = ref StrCheck;
			strCheck55 = (string[])Utils.CopyArray((Array)strCheck55, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR55 = ref StrCheckR;
			strCheckR55 = (string[])Utils.CopyArray((Array)strCheckR55, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck56 = ref StrCheck;
			strCheck56 = (string[])Utils.CopyArray((Array)strCheck56, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR56 = ref StrCheckR;
			strCheckR56 = (string[])Utils.CopyArray((Array)strCheckR56, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФІСКАЛЬНИЙ ЗВІТ ДІЙСНИЙ";
			StrCheckR[StrCheck.Count() - 1] = "";
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", false) == 0))
			{
				ref string[] strCheck57 = ref StrCheck;
				strCheck57 = (string[])Utils.CopyArray((Array)strCheck57, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR57 = ref StrCheckR;
				strCheckR57 = (string[])Utils.CopyArray((Array)strCheckR57, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck58 = ref StrCheck;
			strCheck58 = (string[])Utils.CopyArray((Array)strCheck58, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR58 = ref StrCheckR;
			strCheckR58 = (string[])Utils.CopyArray((Array)strCheckR58, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck59 = ref StrCheck;
			strCheck59 = (string[])Utils.CopyArray((Array)strCheck59, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR59 = ref StrCheckR;
			strCheckR59 = (string[])Utils.CopyArray((Array)strCheckR59, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "РЕГІСТРИ ДЕННИХ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck60 = ref StrCheck;
			strCheck60 = (string[])Utils.CopyArray((Array)strCheck60, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR60 = ref StrCheckR;
			strCheckR60 = (string[])Utils.CopyArray((Array)strCheckR60, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПІДСУМКІВ ОБНУЛЕНІ";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimX(string xmlCheck)
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "X ЗВIT #" + returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string text3 = All.d.GetParametrToString(xmlCheck, "ni", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			bool flag = false;
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr2 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr2, "", false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr2.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric((object)array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr2.ToLower(), "готівка", false) == 0)
				{
					num = num6;
					flag = true;
					text4 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text4, "", false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text5, "", false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text6, "", false) == 0)
					{
						flag = false;
					}
					text7 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text7, "", false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr3, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr3.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "гб", false) == 0))
				{
					string returnStr4 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr4.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr4);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck12 = ref StrCheck;
						strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR12 = ref StrCheckR;
						strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr3.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "дб", false) == 0))
				{
					string returnStr5 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr5.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr5);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck13 = ref StrCheck;
						strCheck13 = (string[])Utils.CopyArray((Array)strCheck13, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR13 = ref StrCheckR;
						strCheckR13 = (string[])Utils.CopyArray((Array)strCheckR13, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray((Array)strCheck14, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray((Array)strCheckR14, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr3.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray((Array)strCheck15, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray((Array)strCheckR15, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr6 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr6, "", false) == 0)
				{
					continue;
				}
				string returnStr7 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr6.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "гб", false) == 0))
				{
					string returnStr8;
					if (Versioned.IsNumeric((object)text2))
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "DTI", returnStr8);
					}
					else
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "TXI", returnStr8);
					}
					if (Operators.CompareString(returnStr8.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray((Array)strCheck16, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray((Array)strCheckR16, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr6.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "дб", false) == 0))
				{
					string returnStr9;
					if (Versioned.IsNumeric((object)text2))
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "DTI", returnStr9);
					}
					else
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "TXI", returnStr9);
					}
					if (Operators.CompareString(returnStr9.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray((Array)strCheck17, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray((Array)strCheckR17, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck18 = ref StrCheck;
				strCheck18 = (string[])Utils.CopyArray((Array)strCheck18, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR18 = ref StrCheckR;
				strCheckR18 = (string[])Utils.CopyArray((Array)strCheckR18, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr6.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray((Array)strCheck19, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray((Array)strCheckR19, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck20 = ref StrCheck;
			strCheck20 = (string[])Utils.CopyArray((Array)strCheck20, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR20 = ref StrCheckR;
			strCheckR20 = (string[])Utils.CopyArray((Array)strCheckR20, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck21 = ref StrCheck;
				strCheck21 = (string[])Utils.CopyArray((Array)strCheck21, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR21 = ref StrCheckR;
				strCheckR21 = (string[])Utils.CopyArray((Array)strCheckR21, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray((Array)strCheck22, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray((Array)strCheckR22, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray((Array)strCheck23, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray((Array)strCheckR23, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray((Array)strCheck24, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray((Array)strCheckR24, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			text3 = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray((Array)strCheck25, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray((Array)strCheckR25, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr10 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr10, "", false) != 0)
				{
					array[i, 0] = returnStr10.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric((object)array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr10.ToLower(), "готівка", false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray((Array)strCheck26, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray((Array)strCheckR26, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray((Array)strCheck27, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray((Array)strCheckR27, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck28 = ref StrCheck;
			strCheck28 = (string[])Utils.CopyArray((Array)strCheck28, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR28 = ref StrCheckR;
			strCheckR28 = (string[])Utils.CopyArray((Array)strCheckR28, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck29 = ref StrCheck;
					strCheck29 = (string[])Utils.CopyArray((Array)strCheck29, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR29 = ref StrCheckR;
					strCheckR29 = (string[])Utils.CopyArray((Array)strCheckR29, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck30 = ref StrCheck;
			strCheck30 = (string[])Utils.CopyArray((Array)strCheck30, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR30 = ref StrCheckR;
			strCheckR30 = (string[])Utils.CopyArray((Array)strCheckR30, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck31 = ref StrCheck;
					strCheck31 = (string[])Utils.CopyArray((Array)strCheck31, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR31 = ref StrCheckR;
					strCheckR31 = (string[])Utils.CopyArray((Array)strCheckR31, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck32 = ref StrCheck;
			strCheck32 = (string[])Utils.CopyArray((Array)strCheck32, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR32 = ref StrCheckR;
			strCheckR32 = (string[])Utils.CopyArray((Array)strCheckR32, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr11 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr11, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr11.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "гб", false) == 0))
				{
					string returnStr12 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr12.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr12);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck33 = ref StrCheck;
						strCheck33 = (string[])Utils.CopyArray((Array)strCheck33, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR33 = ref StrCheckR;
						strCheckR33 = (string[])Utils.CopyArray((Array)strCheckR33, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr11.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "дб", false) == 0))
				{
					string returnStr13 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr13.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(returnStr13);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray((Array)strCheck34, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray((Array)strCheckR34, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck35 = ref StrCheck;
					strCheck35 = (string[])Utils.CopyArray((Array)strCheck35, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR35 = ref StrCheckR;
					strCheckR35 = (string[])Utils.CopyArray((Array)strCheckR35, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr11.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck36 = ref StrCheck;
			strCheck36 = (string[])Utils.CopyArray((Array)strCheck36, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR36 = ref StrCheckR;
			strCheckR36 = (string[])Utils.CopyArray((Array)strCheckR36, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr14 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr14, "", false) == 0)
				{
					continue;
				}
				string returnStr15 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr14.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "гб", false) == 0))
				{
					string text8 = ((!Versioned.IsNumeric((object)text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text8.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(text8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck37 = ref StrCheck;
						strCheck37 = (string[])Utils.CopyArray((Array)strCheck37, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR37 = ref StrCheckR;
						strCheckR37 = (string[])Utils.CopyArray((Array)strCheckR37, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr14.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "дб", false) == 0))
				{
					string text9 = ((!Versioned.IsNumeric((object)text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text9.Trim(), "", false) != 0)
					{
						num4 = All.StrToDouble(text9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck38 = ref StrCheck;
						strCheck38 = (string[])Utils.CopyArray((Array)strCheck38, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR38 = ref StrCheckR;
						strCheckR38 = (string[])Utils.CopyArray((Array)strCheckR38, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray((Array)strCheck39, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray((Array)strCheckR39, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr14.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray((Array)strCheck40, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray((Array)strCheckR40, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray((Array)strCheck41, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray((Array)strCheckR41, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray((Array)strCheck42, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray((Array)strCheckR42, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
				ref string[] strCheck43 = ref StrCheck;
				strCheck43 = (string[])Utils.CopyArray((Array)strCheck43, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR43 = ref StrCheckR;
				strCheckR43 = (string[])Utils.CopyArray((Array)strCheckR43, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text7);
			}
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray((Array)strCheck44, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray((Array)strCheckR44, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr16 = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/z/io").ReturnStr;
			string returnStr17 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/z/io").ReturnStr;
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray((Array)strCheck45, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray((Array)strCheckR45, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr16);
			num += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck46 = ref StrCheck;
			strCheck46 = (string[])Utils.CopyArray((Array)strCheck46, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR46 = ref StrCheckR;
			strCheckR46 = (string[])Utils.CopyArray((Array)strCheckR46, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
			num -= All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			string returnStr18 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr18))
			{
				num -= All.StrToDouble(returnStr18);
			}
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray((Array)strCheck47, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray((Array)strCheckR47, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА У СЕЙФІ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num.ToString());
			ref string[] strCheck48 = ref StrCheck;
			strCheck48 = (string[])Utils.CopyArray((Array)strCheck48, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR48 = ref StrCheckR;
			strCheckR48 = (string[])Utils.CopyArray((Array)strCheckR48, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr19 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr19) && Conversions.ToInteger(returnStr19) > 0)
			{
				ref string[] strCheck49 = ref StrCheck;
				strCheck49 = (string[])Utils.CopyArray((Array)strCheck49, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR49 = ref StrCheckR;
				strCheckR49 = (string[])Utils.CopyArray((Array)strCheckR49, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
				ref string[] strCheck50 = ref StrCheck;
				strCheck50 = (string[])Utils.CopyArray((Array)strCheck50, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR50 = ref StrCheckR;
				strCheckR50 = (string[])Utils.CopyArray((Array)strCheckR50, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr19;
				ref string[] strCheck51 = ref StrCheck;
				strCheck51 = (string[])Utils.CopyArray((Array)strCheck51, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR51 = ref StrCheckR;
				strCheckR51 = (string[])Utils.CopyArray((Array)strCheckR51, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck52 = ref StrCheck;
			strCheck52 = (string[])Utils.CopyArray((Array)strCheck52, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR52 = ref StrCheckR;
			strCheckR52 = (string[])Utils.CopyArray((Array)strCheckR52, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck53 = ref StrCheck;
			strCheck53 = (string[])Utils.CopyArray((Array)strCheck53, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR53 = ref StrCheckR;
			strCheckR53 = (string[])Utils.CopyArray((Array)strCheckR53, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck54 = ref StrCheck;
			strCheck54 = (string[])Utils.CopyArray((Array)strCheck54, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR54 = ref StrCheckR;
			strCheckR54 = (string[])Utils.CopyArray((Array)strCheckR54, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = Tb2;
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimPeriod(string xmlCheck)
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПЕРІОДИЧНИЙ ЗВІТ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "ns", "rq/dat/z").ReturnStr;
			string returnStr3 = All.d.GetParametrToString(xmlCheck, "ds", "rq/dat/z").ReturnStr;
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "З № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "ne", "rq/dat/z").ReturnStr;
			returnStr3 = All.d.GetParametrToString(xmlCheck, "de", "rq/dat/z").ReturnStr;
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ДО № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "all", "rq/dat/z").ReturnStr;
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ВСЬОГО Z ЗВІТІВ";
			StrCheckR[StrCheck.Count() - 1] = returnStr2;
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr4 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr4, "", false) != 0)
				{
					array[i, 0] = returnStr4.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric((object)array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr4.ToLower(), "готівка", false) == 0)
					{
						num = num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck13 = ref StrCheck;
			strCheck13 = (string[])Utils.CopyArray((Array)strCheck13, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR13 = ref StrCheckR;
			strCheckR13 = (string[])Utils.CopyArray((Array)strCheckR13, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray((Array)strCheck14, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray((Array)strCheckR14, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray((Array)strCheck15, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray((Array)strCheckR15, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr5 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr5, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr5.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "гб", false) == 0))
				{
					string returnStr6 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr6.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr6);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray((Array)strCheck16, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray((Array)strCheckR16, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr6);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr5.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "дб", false) == 0))
				{
					string returnStr7 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr7.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr7);
						text = "ОБIГ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray((Array)strCheck17, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray((Array)strCheckR17, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck18 = ref StrCheck;
					strCheck18 = (string[])Utils.CopyArray((Array)strCheck18, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR18 = ref StrCheckR;
					strCheckR18 = (string[])Utils.CopyArray((Array)strCheckR18, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr5.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray((Array)strCheck19, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray((Array)strCheckR19, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr8 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr8, "", false) == 0)
				{
					continue;
				}
				string returnStr9 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr8.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "гб", false) == 0))
				{
					string returnStr10 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
					if (Operators.CompareString(returnStr10.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr10);
						text = "АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck20 = ref StrCheck;
						strCheck20 = (string[])Utils.CopyArray((Array)strCheck20, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR20 = ref StrCheckR;
						strCheckR20 = (string[])Utils.CopyArray((Array)strCheckR20, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr10);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr8.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "дб", false) == 0))
				{
					string returnStr11 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
					if (Operators.CompareString(returnStr11.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr11);
						text = "ПДВ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck21 = ref StrCheck;
						strCheck21 = (string[])Utils.CopyArray((Array)strCheck21, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR21 = ref StrCheckR;
						strCheckR21 = (string[])Utils.CopyArray((Array)strCheckR21, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr11);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray((Array)strCheck22, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray((Array)strCheckR22, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr8.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr9);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray((Array)strCheck23, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray((Array)strCheckR23, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray((Array)strCheck24, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray((Array)strCheckR24, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray((Array)strCheck25, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray((Array)strCheckR25, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray((Array)strCheck26, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray((Array)strCheckR26, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr12 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr12, "", false) != 0)
				{
					array[i, 0] = returnStr12.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric((object)array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr12.ToLower(), "готівка", false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck27 = ref StrCheck;
			strCheck27 = (string[])Utils.CopyArray((Array)strCheck27, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR27 = ref StrCheckR;
			strCheckR27 = (string[])Utils.CopyArray((Array)strCheckR27, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck28 = ref StrCheck;
					strCheck28 = (string[])Utils.CopyArray((Array)strCheck28, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR28 = ref StrCheckR;
					strCheckR28 = (string[])Utils.CopyArray((Array)strCheckR28, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck29 = ref StrCheck;
			strCheck29 = (string[])Utils.CopyArray((Array)strCheck29, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR29 = ref StrCheckR;
			strCheckR29 = (string[])Utils.CopyArray((Array)strCheckR29, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck30 = ref StrCheck;
					strCheck30 = (string[])Utils.CopyArray((Array)strCheck30, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR30 = ref StrCheckR;
					strCheckR30 = (string[])Utils.CopyArray((Array)strCheckR30, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck31 = ref StrCheck;
			strCheck31 = (string[])Utils.CopyArray((Array)strCheck31, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR31 = ref StrCheckR;
			strCheckR31 = (string[])Utils.CopyArray((Array)strCheckR31, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck32 = ref StrCheck;
					strCheck32 = (string[])Utils.CopyArray((Array)strCheck32, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR32 = ref StrCheckR;
					strCheckR32 = (string[])Utils.CopyArray((Array)strCheckR32, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck33 = ref StrCheck;
			strCheck33 = (string[])Utils.CopyArray((Array)strCheck33, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR33 = ref StrCheckR;
			strCheckR33 = (string[])Utils.CopyArray((Array)strCheckR33, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr13 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr13, "", false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr13.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr13.ToLower(), "гб", false) == 0))
				{
					string returnStr14 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr14.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr14);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray((Array)strCheck34, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray((Array)strCheckR34, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr14);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr13.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr13.ToLower(), "дб", false) == 0))
				{
					string returnStr15 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr15.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr15);
						text = "ОБIГ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck35 = ref StrCheck;
						strCheck35 = (string[])Utils.CopyArray((Array)strCheck35, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR35 = ref StrCheckR;
						strCheckR35 = (string[])Utils.CopyArray((Array)strCheckR35, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck36 = ref StrCheck;
					strCheck36 = (string[])Utils.CopyArray((Array)strCheck36, (Array)new string[StrCheck.Count() + 1]);
					ref string[] strCheckR36 = ref StrCheckR;
					strCheckR36 = (string[])Utils.CopyArray((Array)strCheckR36, (Array)new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr13.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck37 = ref StrCheck;
			strCheck37 = (string[])Utils.CopyArray((Array)strCheck37, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR37 = ref StrCheckR;
			strCheckR37 = (string[])Utils.CopyArray((Array)strCheckR37, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr16 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr16, "", false) == 0)
				{
					continue;
				}
				string returnStr17 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr16.ToLower(), "га", false) == 0) | (Operators.CompareString(returnStr16.ToLower(), "гб", false) == 0))
				{
					string returnStr18 = All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr;
					if (Operators.CompareString(returnStr18.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr18);
						text = "АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck38 = ref StrCheck;
						strCheck38 = (string[])Utils.CopyArray((Array)strCheck38, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR38 = ref StrCheckR;
						strCheckR38 = (string[])Utils.CopyArray((Array)strCheckR38, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr16.ToLower(), "да", false) == 0) | (Operators.CompareString(returnStr16.ToLower(), "дб", false) == 0))
				{
					string returnStr19 = All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr;
					if (Operators.CompareString(returnStr19.Trim(), "", false) != 0)
					{
						num4 += All.StrToDouble(returnStr19);
						text = "ПДВ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", false) != 0)
					{
						ref string[] strCheck39 = ref StrCheck;
						strCheck39 = (string[])Utils.CopyArray((Array)strCheck39, (Array)new string[StrCheck.Count() + 1]);
						ref string[] strCheckR39 = ref StrCheckR;
						strCheckR39 = (string[])Utils.CopyArray((Array)strCheckR39, (Array)new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr19);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck40 = ref StrCheck;
				strCheck40 = (string[])Utils.CopyArray((Array)strCheck40, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR40 = ref StrCheckR;
				strCheckR40 = (string[])Utils.CopyArray((Array)strCheckR40, (Array)new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr16.ToLower(), "е", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr16.ToLower(), "ж", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr16.ToLower(), "з", false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric((object)text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray((Array)strCheck41, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray((Array)strCheckR41, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck42 = ref StrCheck;
			strCheck42 = (string[])Utils.CopyArray((Array)strCheck42, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR42 = ref StrCheckR;
			strCheckR42 = (string[])Utils.CopyArray((Array)strCheckR42, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			ref string[] strCheck43 = ref StrCheck;
			strCheck43 = (string[])Utils.CopyArray((Array)strCheck43, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR43 = ref StrCheckR;
			strCheckR43 = (string[])Utils.CopyArray((Array)strCheckR43, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr20 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			string returnStr21 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr20) && Conversions.ToInteger(returnStr20) > 0)
			{
				ref string[] strCheck44 = ref StrCheck;
				strCheck44 = (string[])Utils.CopyArray((Array)strCheck44, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR44 = ref StrCheckR;
				strCheckR44 = (string[])Utils.CopyArray((Array)strCheckR44, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr21);
				ref string[] strCheck45 = ref StrCheck;
				strCheck45 = (string[])Utils.CopyArray((Array)strCheck45, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR45 = ref StrCheckR;
				strCheckR45 = (string[])Utils.CopyArray((Array)strCheckR45, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr20;
				ref string[] strCheck46 = ref StrCheck;
				strCheck46 = (string[])Utils.CopyArray((Array)strCheck46, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR46 = ref StrCheckR;
				strCheckR46 = (string[])Utils.CopyArray((Array)strCheckR46, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray((Array)strCheck47, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray((Array)strCheckR47, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck48 = ref StrCheck;
			strCheck48 = (string[])Utils.CopyArray((Array)strCheck48, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR48 = ref StrCheckR;
			strCheckR48 = (string[])Utils.CopyArray((Array)strCheckR48, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck49 = ref StrCheck;
			strCheck49 = (string[])Utils.CopyArray((Array)strCheck49, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR49 = ref StrCheckR;
			strCheckR49 = (string[])Utils.CopyArray((Array)strCheckR49, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = Tb2;
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoAll(string xmlCheck, string OnOf = "онлайн")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray((Array)strCheck, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray((Array)strCheckR, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "t", "rq/dat/c").ReturnStr;
			int num = 0;
			if (Versioned.IsNumeric((object)returnStr))
			{
				num = Conversions.ToInteger(returnStr);
			}
			if (num > 100)
			{
				num -= 100;
			}
			returnStr = num.ToString();
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray((Array)strCheck2, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray((Array)strCheckR2, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			switch (returnStr)
			{
			case "8":
			{
				ref string[] strCheck7 = ref StrCheck;
				strCheck7 = (string[])Utils.CopyArray((Array)strCheck7, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR7 = ref StrCheckR;
				strCheckR7 = (string[])Utils.CopyArray((Array)strCheckR7, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ВІДКРИТТЯ ЗМІНИ ПРРО";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "12":
			{
				ref string[] strCheck6 = ref StrCheck;
				strCheck6 = (string[])Utils.CopyArray((Array)strCheck6, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR6 = ref StrCheckR;
				strCheckR6 = (string[])Utils.CopyArray((Array)strCheckR6, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАПИТ ДІАПАЗОНУ РЕЗЕРВНИХ НОМЕРІВ ДЛЯ РОБОТИ В РЕЖИМІ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "9":
			{
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray((Array)strCheck5, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray((Array)strCheckR5, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОЧАТОК ПЕРЕВЕДЕННЯ ПРРО В РЕЖИМ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "10":
			{
				ref string[] strCheck4 = ref StrCheck;
				strCheck4 = (string[])Utils.CopyArray((Array)strCheck4, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR4 = ref StrCheckR;
				strCheckR4 = (string[])Utils.CopyArray((Array)strCheckR4, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАВЕРШЕННЯ РЕЖИМУ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			default:
			{
				ref string[] strCheck3 = ref StrCheck;
				strCheck3 = (string[])Utils.CopyArray((Array)strCheck3, (Array)new string[StrCheck.Count() + 1]);
				ref string[] strCheckR3 = ref StrCheckR;
				strCheckR3 = (string[])Utils.CopyArray((Array)strCheckR3, (Array)new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			}
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray((Array)strCheck8, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray((Array)strCheckR8, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = Tb2;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray((Array)strCheck9, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray((Array)strCheckR9, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck10 = ref StrCheck;
			strCheck10 = (string[])Utils.CopyArray((Array)strCheck10, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR10 = ref StrCheckR;
			strCheckR10 = (string[])Utils.CopyArray((Array)strCheckR10, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray((Array)strCheck11, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray((Array)strCheckR11, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck12 = ref StrCheck;
			strCheck12 = (string[])Utils.CopyArray((Array)strCheck12, (Array)new string[StrCheck.Count() + 1]);
			ref string[] strCheckR12 = ref StrCheckR;
			strCheckR12 = (string[])Utils.CopyArray((Array)strCheckR12, (Array)new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "С Л У Ж Б О В И Й   Ч Е К";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private bool ExportToPDFnew(string PathPDF, string TaxNcheck)
	{
		bool result;
		checked
		{
			try
			{
				iTextSharp.text.Rectangle pageSize = new iTextSharp.text.Rectangle(243f, 841f);
				Document document = new Document(pageSize);
				string name = Environment.GetFolderPath(Environment.SpecialFolder.Fonts) + "\\consola.ttf";
				if (!File.Exists(PathPDF))
				{
					BaseFont bf = BaseFont.CreateFont(name, "CP1251", embedded: true);
					Font font = new Font(bf, 10f, 0);
					QrCodeImgControl qrCodeImgControl = new QrCodeImgControl();
					((Control)qrCodeImgControl).Width = 210;
					((Control)qrCodeImgControl).Height = 210;
					qrCodeImgControl.Text = CheckVis(TaxNcheck);
					ImageQRt = ((PictureBox)qrCodeImgControl).Image;
					Image instance = Image.GetInstance(ImageQRt, ImageFormat.Bmp);
					int num = LoadImg();
					Image image = num switch
					{
						1 => Image.GetInstance(PrintLogo, ImageFormat.Bmp), 
						2 => Image.GetInstance(PrintLogo, ImageFormat.Jpeg), 
						3 => Image.GetInstance(PrintLogo, ImageFormat.Png), 
						_ => null, 
					};
					PdfWriter.GetInstance(document, new FileStream(PathPDF, FileMode.Create));
					document.Open();
					if (num > 0)
					{
						image.SetAbsolutePosition(9f, 774f);
						image.ScaleAbsoluteWidth(225f);
						image.ScaleAbsoluteHeight(57f);
						document.Add(image);
						document.Add(new Paragraph(" "));
						document.Add(new Paragraph(" "));
					}
					int num2 = StrCheckN.Count() - 1;
					for (int i = 0; i <= num2; i++)
					{
						if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", false) != 0)
						{
							document.Add(new Paragraph(StrCheckN[i], font));
							continue;
						}
						instance.ScaleAbsoluteWidth(168f);
						instance.ScaleAbsoluteHeight(168f);
						document.Add(instance);
					}
					document.Close();
					goto IL_01fb;
				}
				result = false;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
			goto IL_01fd;
		}
		IL_01fb:
		result = true;
		goto IL_01fd;
		IL_01fd:
		return result;
	}

	private bool ExportToPDF(string PathPDF)
	{
		bool result;
		checked
		{
			try
			{
				Document document = new Document();
				string name = Environment.GetFolderPath(Environment.SpecialFolder.Fonts) + "\\consola.ttf";
				if (!File.Exists(PathPDF))
				{
					BaseFont bf = BaseFont.CreateFont(name, "CP1251", embedded: true);
					Font font = new Font(bf, 10f, 0);
					PdfWriter.GetInstance(document, new FileStream(PathPDF, FileMode.Create));
					document.Open();
					int num = StrCheckN.Count() - 1;
					for (int i = 0; i <= num; i++)
					{
						if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", false) != 0)
						{
							document.Add(new Paragraph(StrCheckN[i], font));
						}
					}
					document.Close();
					goto IL_00c1;
				}
				result = false;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
			goto IL_00c3;
		}
		IL_00c1:
		result = true;
		goto IL_00c3;
		IL_00c3:
		return result;
	}

	private int LoadImg()
	{
		int result = 0;
		try
		{
			string text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".bmp";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				result = 1;
			}
			else
			{
				text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".jpg";
				if (File.Exists(text))
				{
					PrintLogo = Image.FromFile(text);
					result = 2;
				}
				else
				{
					text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".png";
					if (File.Exists(text))
					{
						PrintLogo = Image.FromFile(text);
						result = 3;
					}
					else
					{
						text = All.MyDoc() + "\\WebCheck\\logo.png";
						if (File.Exists(text))
						{
							PrintLogo = Image.FromFile(text);
							result = 3;
						}
					}
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool ExportToTXT(string PathTXT)
	{
		bool result;
		checked
		{
			try
			{
				if (!File.Exists(PathTXT))
				{
					StreamWriter streamWriter = new StreamWriter(PathTXT, append: true);
					int num = StrCheckN.Count() - 1;
					for (int i = 0; i <= num; i++)
					{
						streamWriter.WriteLine(StrCheckN[i]);
					}
					streamWriter.Flush();
					streamWriter.Close();
					Application.DoEvents();
					goto IL_0060;
				}
				result = false;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
			goto IL_0062;
		}
		IL_0060:
		result = true;
		goto IL_0062;
		IL_0062:
		return result;
	}

	private bool ExportToArrayS()
	{
		bool result;
		checked
		{
			try
			{
				Secondary.SetSizeLastCheckLine(StrCheckN.Count() - 1);
				string text = "";
				int num = StrCheckN.Count() - 1;
				for (int i = 0; i <= num; i++)
				{
					Secondary.SetLastCheckLine(i, StrCheckN[i]);
					text = ((i >= StrCheckN.Count() - 1) ? (text + StrCheckN[i]) : (text + StrCheckN[i] + "\r\n"));
				}
				if (Secondary.SendMail[0].Trim().Length > 3)
				{
					Secondary.SendMail[1] = text;
					Secondary.SendMail[4] = "https://cabinet.tax.gov.ua/cashregs/check?id=" + Secondary.SendMail[2] + "&date=" + Secondary.SendMail[3] + "&fn=" + FnPr + "&sm=" + SumPr;
				}
				else
				{
					Secondary.SendMail[0] = "";
					Secondary.SendMail[1] = "";
					Secondary.SendMail[4] = "";
				}
				Application.DoEvents();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_0129;
			}
			result = true;
			goto IL_0129;
		}
		IL_0129:
		return result;
	}
}
