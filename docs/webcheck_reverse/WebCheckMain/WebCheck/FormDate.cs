using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormDate : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	private string xmlPeriod;

	private TypTaxReport[] T;

	private TypPayReport[] P;

	private int NcNi;

	private int NcNo;

	public int zEPC;

	public double zEPSM;

	private int zCount;

	private string zStart;

	private string zFinish;

	private string zStartD;

	private string zFinishD;

	private string zStartT;

	private string zFinishT;

	private bool SSS;

	private double SMIM;

	private double SMIP;

	private double SMOM;

	private double SMOP;

	[field: AccessedThroughProperty("DateTime1")]
	internal virtual DateTimePicker DateTime1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DateTime2")]
	internal virtual DateTimePicker DateTime2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Indi")]
	internal virtual TextBox Indi
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_00f5: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ff: Expected O, but got Unknown
		//IL_017a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0184: Expected O, but got Unknown
		//IL_020e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0218: Expected O, but got Unknown
		//IL_0305: Unknown result type (might be due to invalid IL or missing references)
		//IL_030f: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormDate));
		DateTime1 = new DateTimePicker();
		DateTime2 = new DateTimePicker();
		NoB = new Button();
		OkB = new Button();
		Indi = new TextBox();
		((Control)this).SuspendLayout();
		((Control)DateTime1).Location = new Point(16, 78);
		((Control)DateTime1).Name = "DateTime1";
		((Control)DateTime1).Size = new Size(200, 22);
		((Control)DateTime1).TabIndex = 0;
		((Control)DateTime2).Location = new Point(274, 78);
		((Control)DateTime2).Name = "DateTime2";
		((Control)DateTime2).Size = new Size(200, 22);
		((Control)DateTime2).TabIndex = 1;
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(16, 123);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(132, 40);
		((Control)NoB).TabIndex = 22;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(342, 123);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(132, 40);
		((Control)OkB).TabIndex = 21;
		((ButtonBase)OkB).Text = "Створити";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)Indi).Enabled = false;
		((Control)Indi).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Indi).Location = new Point(16, 23);
		((Control)Indi).Name = "Indi";
		((Control)Indi).Size = new Size(458, 30);
		((Control)Indi).TabIndex = 23;
		Indi.TextAlign = (HorizontalAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(491, 184);
		((Control)this).Controls.Add((Control)(object)Indi);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)DateTime2);
		((Control)this).Controls.Add((Control)(object)DateTime1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormDate";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Вибір періоду";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormDate()
	{
		((Form)this).Load += FormDate_Load;
		SSS = false;
		InitializeComponent();
		checked
		{
			T = new TypTaxReport[All.PayTax.TaxN + 1];
			ref TypPayReport[] p = ref P;
			p = (TypPayReport[])Utils.CopyArray((Array)p, (Array)new TypPayReport[All.PayTax.PayN + 1]);
		}
	}

	private void Zero()
	{
		xmlPeriod = "";
		zCount = 0;
		NcNi = 0;
		NcNo = 0;
		zStart = "";
		zFinish = "";
		zStartD = "";
		zFinishD = "";
		zStartT = "";
		zFinishT = "";
		zEPC = 0;
		zEPSM = 0.0;
		int taxN = All.PayTax.TaxN;
		checked
		{
			for (int i = 0; i <= taxN; i++)
			{
				ref TypTaxReport reference = ref T[i];
				reference.Name = All.PayTax.get_TaxName(i);
				reference.SMI = 0.0;
				reference.SMO = 0.0;
				reference.DTI = 0.0;
				reference.DTO = 0.0;
				reference.TXI = 0.0;
				reference.TXO = 0.0;
				reference.TXPR = All.StrToDouble(All.PayTax.get_TaxPRC(i));
				reference.DTPR = All.StrToDouble(All.PayTax.get_TaxEXCISE(i));
				reference.Visible = false;
			}
			int payN = All.PayTax.PayN;
			for (int i = 0; i <= payN; i++)
			{
				ref TypPayReport reference2 = ref P[i];
				reference2.Name = All.PayTax.get_PayName(i);
				reference2.SMI = 0.0;
				reference2.SMO = 0.0;
			}
			SSS = false;
			SMIM = 0.0;
			SMIP = 0.0;
			SMOM = 0.0;
			SMOP = 0.0;
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void FormDate_Load(object sender, EventArgs e)
	{
		int integer = All.f.GetInteger("Global", "FormPrintY", 0);
		if (integer > 0)
		{
			int integer2 = All.f.GetInteger("Global", "FormPrintX", 0);
			((Control)this).Top = integer;
			((Control)this).Left = integer2;
		}
		((Form)this).Text = "Періодичний ЗВІТ";
		Zero();
		Indi.Text = "Вкажіть потрібний період...";
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		//IL_0200: Unknown result type (might be due to invalid IL or missing references)
		((Control)OkB).Enabled = false;
		Zero();
		DateTime value = DateTime1.Value;
		string text = value.Year + "-" + All.TwoS(value.Month.ToString()) + "-" + All.TwoS(value.Day.ToString());
		text += "  00:00:00";
		DateTime value2 = DateTime2.Value;
		string text2 = value2.Year + "-" + All.TwoS(value2.Month.ToString()) + "-" + All.TwoS(value2.Day.ToString());
		text2 += "  23:59:59";
		TypErrStr typErrStr = ReprtPeriod(text, text2);
		if (typErrStr.errCode > 0)
		{
			Indi.Text = "Виникла помилка. Дивіться лог файл.";
			All.Lg.SaveTextToLog("PeriodReport", typErrStr.errStr, "Код ошибки: " + typErrStr.errCode);
			((Control)OkB).Enabled = true;
			return;
		}
		if (zCount < 1)
		{
			Indi.Text = "За вказаний період звітів немає.";
			((Control)OkB).Enabled = true;
			return;
		}
		Payment();
		if (!XmlPeriodReport())
		{
			Indi.Text = "Виникла помилка. Дивіться лог файл.";
			All.Lg.SaveTextToLog("PeriodReport", "Отчет не может быть показан", "Ошибка при создании XML документа");
			((Control)OkB).Enabled = true;
		}
		else
		{
			Indi.Text = "Звіт створено.";
			All.Lg.SaveTextToLog("Звіт за період", xmlPeriod);
			((Form)new FormPrint("", xmlPeriod, 1)).ShowDialog();
			Indi.Text = "Вкажіть потрібний період...";
			((Control)OkB).Enabled = true;
		}
	}

	private void Payment()
	{
		Directorys payTax = All.PayTax;
		int taxN = payTax.TaxN;
		for (int i = 1; i <= taxN; i = checked(i + 1))
		{
			if ((T[i].SMI > 0.0) | (T[i].SMO > 0.0))
			{
				if (T[i].Name.Trim().Length == 1)
				{
					T[i].TXI = All.TaxAmountBig(T[i].SMI, T[i].TXPR);
					T[i].TXO = All.TaxAmountBig(T[i].SMO, T[i].TXPR);
				}
				else if (Operators.CompareString(Conversions.ToString(T[i].Name[0]).ToLower(), "д", false) == 0)
				{
					double num = All.TaxAmountBig(T[i].SMI, 27.5);
					double amount = T[i].SMI - num;
					T[i].DTI = All.TaxAmountr(amount, 7.5);
					double num2 = All.TaxAmountBig(T[i].SMO, 27.5);
					double amount2 = T[i].SMO - num2;
					T[i].DTO = All.TaxAmountr(amount2, 7.5);
					All.TaxAmountr(amount, 20.0);
					double tXI = All.TaxAmountr(amount, 20.0);
					All.TaxAmountr(amount2, 20.0);
					double tXO = All.TaxAmountr(amount2, 20.0);
					T[i].TXI = tXI;
					T[i].TXO = tXO;
					string name = T[i].Name;
					name = Conversions.ToString(name[1]);
					int num3 = Conversions.ToInteger(payTax.ABCtoNUM(name));
					T[num3].TXI += T[i].TXI;
					T[num3].TXO += T[i].TXO;
					T[num3].Visible = true;
					T[i].Visible = true;
				}
				else
				{
					T[i].DTI = All.TaxAmountBig(T[i].SMI, T[i].DTPR);
					T[i].DTO = All.TaxAmountBig(T[i].SMO, T[i].DTPR);
					string name2 = T[i].Name;
					name2 = Conversions.ToString(name2[1]);
					int num4 = Conversions.ToInteger(payTax.ABCtoNUM(name2));
					double amount3 = T[i].SMI - T[i].DTI;
					double amount4 = T[i].SMO - T[i].DTO;
					T[i].TXI = All.TaxAmountBig(amount3, T[num4].TXPR);
					T[i].TXO = All.TaxAmountBig(amount4, T[num4].TXPR);
					T[num4].TXI += T[i].TXI;
					T[num4].TXO += T[i].TXO;
					T[num4].Visible = true;
					T[i].Visible = true;
				}
			}
		}
		payTax = null;
	}

	private bool XmlPeriodReport()
	{
		string text = "";
		long num = All.СurrentCompDate();
		text = "";
		text = text + "<DAT FN='" + All.A.FN + "' TN='" + All.A.TIN + "' DI='WebCheck' ZN='0' V='1'>";
		text = text + "<Z NO='" + DatZapros() + "' DS='" + zStartD + "' DE='" + zFinishD + "' ALL='" + zCount + "' NS='" + zStart + "' NE='" + zFinish + "'>";
		_ = All.PayTax;
		int payN = All.PayTax.PayN;
		checked
		{
			for (int i = 0; i <= payN; i++)
			{
				if (!((P[i].SMI > 0.0) | (P[i].SMO > 0.0)))
				{
					continue;
				}
				string text2 = i.ToString();
				if (Operators.CompareString(text2.Trim(), "1", false) == 0)
				{
					text2 = "0";
				}
				string text3 = "";
				if (Operators.CompareString(text2, "0", false) == 0)
				{
					if (SSS)
					{
						text3 = "' SMIM='" + All.Bablo(SMIM) + "' SMIP='" + All.Bablo(SMIP) + "' SMOM='" + All.Bablo(SMOM) + "' SMOP='" + All.Bablo(SMOP);
					}
				}
				else
				{
					text3 = "";
				}
				text = text + "<M T='" + text2 + "' NM='" + P[i].Name + "' SMI='" + P[i].SMI + "' SMO='" + P[i].SMO + text3 + "'/>";
			}
			_ = null;
			Directorys payTax = All.PayTax;
			int taxN = payTax.TaxN;
			for (int i = 1; i <= taxN; i++)
			{
				if ((T[i].SMI > 0.0) | (T[i].SMO > 0.0) | (T[i].TXI > 0.0) | (T[i].TXO > 0.0) | T[i].Visible)
				{
					payTax.ABCtoNUM(T[i].Name);
					_ = DateTime.Now;
					text = ((T[i].Name.Trim().Length != 1) ? (text + "<TXS N='" + All.PayTax.get_TaxName(i) + "' TXPR='" + All.Bablo(All.PayTax.get_TaxPRC(i)) + "' TXI='" + All.Bablo(T[i].TXI) + "' TXO='" + All.Bablo(T[i].TXO) + "' SMI='" + All.Bablo(T[i].SMI) + "' SMO='" + All.Bablo(T[i].SMO) + "' DTPR='" + All.Bablo(T[i].DTPR) + "' DTI='" + All.Bablo(T[i].DTI) + "' DTO='" + All.Bablo(T[i].DTO) + "' TXTY='0' TXAL='0'/>") : (text + "<TXS N='" + All.PayTax.get_TaxName(i) + "' TXPR='" + All.Bablo(All.PayTax.get_TaxPRC(i)) + "' TXI='" + All.Bablo(T[i].TXI) + "' TXO='" + All.Bablo(T[i].TXO) + "' SMI='" + All.Bablo(T[i].SMI) + "' SMO='" + All.Bablo(T[i].SMO) + "' DTPR='0.00' DTI='0.00' DTO='0' TXTY='0' TXAL='0'/>"));
				}
			}
			payTax = null;
			text = text + "<EPZ EPC='" + zEPC + "' EPCS='0' EPSM='" + zEPSM + "'></EPZ>";
			text += "</Z>";
			text = text + "<TS>" + num + "</TS>";
			text += "</DAT>";
			text = "<RQ V='1'>" + text + "</RQ>";
			if (All.d.VerifyXML(text))
			{
				xmlPeriod = text;
				return true;
			}
			if (Operators.CompareString(text.Trim(), "", false) == 0)
			{
				text = "XML не может быть пустой строкой";
			}
			All.Lg.SaveTextToLog("PeriodReport", "Ошибка XML", text);
			xmlPeriod = "";
			return false;
		}
	}

	private string DatZapros()
	{
		string text = "ВIД " + All.TwoS(DateTime1.Value.Day.ToString()) + "." + All.TwoS(DateTime1.Value.Month.ToString()) + "." + DateTime1.Value.Year;
		return text + " ДО " + All.TwoS(DateTime2.Value.Day.ToString()) + "." + All.TwoS(DateTime2.Value.Month.ToString()) + "." + DateTime2.Value.Year;
	}

	internal TypErrStr ReprtPeriod(string d1, string d2)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		checked
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT checkxml FROM ksef WHERE Datetime(dt)>=Datetime('" + d1 + "') AND Datetime(dt)<=Datetime('" + d2 + "') AND DocType='80'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					string text = sQLiteDataReader[0].ToString();
					zCount++;
					if (!DerebanXmlZ(text))
					{
						result.errCode = 68;
						result.errStr = "Ошибка открытия одного из Z отчетов";
						result.ReturnStr = "";
						All.Lg.SaveTextToLog("ReprtPeriod", "Ошибка разбора Z отчета:", text);
						return result;
					}
					Indi.Text = "Обробка звіту №" + zFinish.ToString();
					Application.DoEvents();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errCode = 68;
				result.errStr = "Ошибка формирования переодического отчета";
				result.ReturnStr = "";
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}

	private bool DerebanXmlZ(string XmlZ)
	{
		XmlDocument xmlDocument = new XmlDocument();
		bool result;
		try
		{
			xmlDocument.LoadXml(XmlZ.ToLower());
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_05e2;
		}
		if (Operators.CompareString(zStart.Trim(), "", false) == 0)
		{
			zStart = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z").ReturnStr;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			zStartD = LongToData(innerText);
			zStartT = LongToTime(innerText);
			zFinish = zStart;
			zFinishD = zStartD;
			zFinishT = zStartT;
		}
		else
		{
			zFinish = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z").ReturnStr;
			string innerText2 = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			zFinishD = LongToData(innerText2);
			zFinishT = LongToTime(innerText2);
		}
		string returnStr = All.d.GetParametrToString(XmlZ, "ni", "rq/dat/z/nc").ReturnStr;
		string returnStr2 = All.d.GetParametrToString(XmlZ, "no", "rq/dat/z/nc").ReturnStr;
		checked
		{
			if (Versioned.IsNumeric((object)returnStr))
			{
				NcNi += Conversions.ToInteger(returnStr);
			}
			if (Versioned.IsNumeric((object)returnStr2))
			{
				NcNo += Conversions.ToInteger(returnStr2);
			}
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num2 = num;
			for (int i = 0; i <= num2; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr3, "", false) != 0)
				{
					string text = returnStr3.ToUpper().Trim();
					int num3 = All.PayTax.get_PayABCtoNum(text);
					if (num3 < 0)
					{
						num3 = All.PayTax.AddPayTemp(text);
						ref TypPayReport[] p = ref P;
						p = (TypPayReport[])Utils.CopyArray((Array)p, (Array)new TypPayReport[All.PayTax.PayN + 1]);
						P[All.PayTax.PayN].Name = text;
						P[All.PayTax.PayN].SMI = 0.0;
						P[All.PayTax.PayN].SMO = 0.0;
					}
					double num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr);
					double num5 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr);
					P[num3].SMI += num4;
					P[num3].SMO += num5;
					SMIM += All.StrToDouble(All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr);
					if (SMIM > 0.0)
					{
						SSS = true;
					}
					SMIP += All.StrToDouble(All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr);
					if (SMIP > 0.0)
					{
						SSS = true;
					}
					SMOM += All.StrToDouble(All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr);
					if (SMOM > 0.0)
					{
						SSS = true;
					}
					SMOP += All.StrToDouble(All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr);
					if (SMOP > 0.0)
					{
						SSS = true;
					}
				}
			}
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			int num6 = elementsByTagName.Count - 1;
			for (int i = 0; i <= num6; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr4 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr4, "", false) != 0)
				{
					string nameTaxe = returnStr4.ToUpper().Trim();
					int taxIndex = All.PayTax.Search(nameTaxe).TaxIndex;
					double num7 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					double num8 = All.StrToDouble(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					T[taxIndex].SMI += num7;
					T[taxIndex].SMO += num8;
				}
			}
			string returnStr5 = All.d.GetParametrToString(XmlZ, "epc", "rq/dat/z/epz").ReturnStr;
			string returnStr6 = All.d.GetParametrToString(XmlZ, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric((object)returnStr5))
			{
				zEPC += Conversions.ToInteger(returnStr5);
				zEPSM += All.StrToDouble(returnStr6);
			}
			result = true;
			goto IL_05e2;
		}
		IL_05e2:
		return result;
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

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}
}
